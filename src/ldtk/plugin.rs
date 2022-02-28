use super::data::LdtkData;
use crate::debug::DebugTarget;
use anyhow::{Context, Result};
use bevy::{
    asset::{AssetLoader, LoadContext, LoadedAsset},
    prelude::*,
    reflect::TypeUuid,
    utils::BoxedFuture,
};
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::{prelude::*, rapier::parry::transformation::vhacd::VHACDParameters};
use geo_booleanop::boolean::BooleanOp;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};

const Z_COLLISION: f32 = 10.0;
const COLLIDER_MATERIAL: ColliderMaterial = ColliderMaterial {
    friction: 0.0,
    restitution: 0.0,
    friction_combine_rule: CoefficientCombineRule::Max,
    restitution_combine_rule: CoefficientCombineRule::Min,
};

pub struct LdtkPlugin;
impl Plugin for LdtkPlugin {
    fn build(&self, app: &mut App) {
        app.add_asset::<Ldtk>()
            .init_asset_loader::<LdtkLoader>()
            .add_event::<LdtkEvent>()
            .add_system(on_asset_event_system);
    }
}

#[derive(Debug, Deserialize, TypeUuid)]
#[uuid = "070d77d1-b60d-4ce9-a16f-5492c1c0548e"]
pub struct Ldtk {
    pub file_path: PathBuf,
    pub data: LdtkData,
}

#[derive(Debug)]
pub enum LdtkEvent {
    SpawnPlayer(Vec3),
    SpawnEnemy { name: String, position: Vec3 },
}

impl Ldtk {
    fn load(
        &self,
        level_identifier: &str,
        asset_server: &Res<AssetServer>,
        texture_atlases: &mut ResMut<Assets<TextureAtlas>>,
        commands: &mut Commands,
        rapier_config: &Res<RapierConfiguration>,
        event_writer: &mut EventWriter<LdtkEvent>,
    ) -> Result<()> {
        let level = self
            .data
            .levels
            .iter()
            .find(|level| level.identifier == level_identifier)
            .with_context(|| format!("identifier {} not found", level_identifier))?;

        let layer_instances = level
            .layer_instances
            .as_ref()
            .with_context(|| format!("{} has no layers", level_identifier))?;

        // tileset
        let mut tileset_defs = layer_instances
            .iter()
            .filter_map(|layer_instance| layer_instance.tileset_def_uid)
            .filter_map(|tileset_def_uid| {
                self.data
                    .defs
                    .tilesets
                    .iter()
                    .find(|tileset| tileset.uid == tileset_def_uid)
            })
            .collect::<Vec<_>>();
        // make unique
        tileset_defs.sort_by(|a, b| a.uid.cmp(&b.uid));
        tileset_defs.dedup_by(|a, b| a.uid == b.uid);

        // create texture atlas
        let base_path = self
            .file_path
            .parent()
            .with_context(|| format!("failed to get parent directory, {:?}", self.file_path))?;

        let texture_atlas_handles = tileset_defs
            .iter()
            .map(|tileset_def| {
                let tile_size = Vec2::splat(tileset_def.tile_grid_size as f32);

                let mut texture_path = std::path::PathBuf::new();
                texture_path.push(base_path);
                texture_path.push(tileset_def.rel_path.clone());

                let texture_handle = asset_server.load(texture_path.as_path());
                let texture_atlas = TextureAtlas::from_grid(
                    texture_handle,
                    tile_size,
                    tileset_def.c_wid as usize,
                    tileset_def.c_hei as usize,
                );
                let texture_atlas_handle = texture_atlases.add(texture_atlas);
                (tileset_def.uid, texture_atlas_handle)
            })
            .collect::<HashMap<_, _>>();

        // get tileset collision data
        let tileset_collisions = tileset_defs
            .iter()
            .map(|tileset_def| {
                let tileset_collision = tileset_def
                    .custom_data
                    .iter()
                    .filter_map(|custom_data| {
                        let tile_id = custom_data.get("tileId").and_then(|value| {
                            if let Some(serde_json::Value::Number(value)) = value.as_ref() {
                                value.as_i64()
                            } else {
                                None
                            }
                        });
                        let data = custom_data
                            .get("data")
                            .and_then(|value| {
                                if let Some(serde_json::Value::String(value)) = value.as_ref() {
                                    serde_json::from_str::<Vec<(f32, f32)>>(value).ok()
                                } else {
                                    None
                                }
                            })
                            .map(|data| {
                                data.into_iter()
                                    .map(|(x, y)| {
                                        Vec2::new(x, -y) * tileset_def.tile_grid_size as f32
                                    })
                                    .collect::<Vec<_>>()
                            });
                        if tile_id.is_none() || data.is_none() {
                            None
                        } else {
                            Some((tile_id.unwrap(), data.unwrap()))
                        }
                    })
                    .collect::<HashMap<_, _>>();
                (tileset_def.uid, tileset_collision)
            })
            .collect::<HashMap<_, _>>();

        let level_position = Vec3::new(level.world_x as f32, -level.world_y as f32, 0.0);

        // layers
        for layer_instance in layer_instances {
            match layer_instance.layer_instance_type.as_str() {
                "Entities" => {
                    for entity_instance in &layer_instance.entity_instances {
                        let position = Vec3::new(
                            entity_instance.px[0] as f32,
                            -entity_instance.px[1] as f32,
                            0.0,
                        ) + level_position;
                        match entity_instance.identifier.as_str() {
                            "PlayerStart" => {
                                event_writer.send(LdtkEvent::SpawnPlayer(position));
                            }
                            "Enemy" => {
                                let name = entity_instance
                                    .field_instances
                                    .iter()
                                    .find(|field_instance| field_instance.identifier == "name")
                                    .and_then(|field_instance| field_instance.value.as_ref())
                                    .and_then(|field| field.as_str())
                                    .map(|s| s.to_string())
                                    .with_context(|| {
                                        format!(
                                            "no name field: {:?}",
                                            entity_instance.field_instances
                                        )
                                    })?;
                                event_writer.send(LdtkEvent::SpawnEnemy { name, position });
                            }
                            _ => {}
                        }
                    }
                }
                "Tiles" if layer_instance.tileset_def_uid.is_some() => {
                    let tileset_def_uid = layer_instance.tileset_def_uid.unwrap();
                    let texture_atlas_handle = texture_atlas_handles
                        .get(&tileset_def_uid)
                        .with_context(|| {
                            format!("failed to find tile identifier: {}", tileset_def_uid)
                        })?;

                    let grid_tile_offset = Vec3::new(
                        layer_instance.grid_size as f32,
                        -layer_instance.grid_size as f32,
                        0.0,
                    ) * 0.5;

                    // create collision bundles with debug geometry
                    let collisions = tileset_collisions
                        .get(&tileset_def_uid)
                        .and_then(|tileset_collision| {
                            let polygons = layer_instance
                                .grid_tiles
                                .iter()
                                .filter_map(|grid_tile| {
                                    let grid_tile_position =
                                        Vec2::new(grid_tile.px[0] as f32, -grid_tile.px[1] as f32);
                                    tileset_collision.get(&grid_tile.t).map(|collision| {
                                        collision
                                            .iter()
                                            .map(|v| *v + grid_tile_position)
                                            .collect::<Vec<_>>()
                                    })
                                })
                                .collect::<Vec<_>>();
                            merge_polygons(&polygons)
                        })
                        .map(|polygons| {
                            polygons
                                .into_iter()
                                .map(|polygon| {
                                    let vertices = polygon
                                        .iter()
                                        .map(|v| point!(v.x, v.y) / rapier_config.scale)
                                        .collect::<Vec<_>>();
                                    let indices = (0..polygon.len()).collect::<Vec<_>>();
                                    let mut indices = indices
                                        .iter()
                                        .zip(indices.iter().skip(1))
                                        .map(|(a, b)| [*a as u32, *b as u32])
                                        .collect::<Vec<_>>();
                                    indices.push([polygon.len() as u32 - 1, 0]);
                                    (
                                        ColliderBundle {
                                            shape: ColliderShape::convex_decomposition_with_params(
                                                vertices.as_slice(),
                                                indices.as_slice(),
                                                &VHACDParameters {
                                                    concavity: 0.0025,
                                                    //convex_hull_approximation: false,
                                                    ..Default::default()
                                                },
                                            )
                                            .into(),
                                            material: COLLIDER_MATERIAL.into(),
                                            position: (level_position / rapier_config.scale).into(),
                                            ..Default::default()
                                        },
                                        GeometryBuilder::build_as(
                                            &shapes::Polygon {
                                                points: polygon,
                                                closed: true,
                                            },
                                            DrawMode::Outlined {
                                                fill_mode: FillMode::color(Color::rgba(
                                                    1.0, 1.0, 1.0, 0.2,
                                                )),
                                                outline_mode: StrokeMode::new(
                                                    Color::rgba(1.0, 1.0, 1.0, 1.0),
                                                    1.0,
                                                ),
                                            },
                                            Transform::from_xyz(0.0, 0.0, Z_COLLISION),
                                        ),
                                    )
                                })
                                .collect::<Vec<_>>()
                        });

                    // spawn layer
                    commands
                        .spawn()
                        .insert(ColliderPositionComponent(
                            ColliderPosition::from(level_position / rapier_config.scale).into(),
                        ))
                        .insert(ColliderPositionSync::Discrete)
                        .insert(GlobalTransform::identity())
                        .with_children(|parent| {
                            // spawn tiles
                            for grid_tile in &layer_instance.grid_tiles {
                                let grid_tile_position =
                                    Vec3::new(grid_tile.px[0] as f32, -grid_tile.px[1] as f32, 1.0)
                                        + grid_tile_offset;
                                let transform = Transform::from_translation(grid_tile_position);
                                parent.spawn_bundle(SpriteSheetBundle {
                                    texture_atlas: texture_atlas_handle.clone(),
                                    sprite: TextureAtlasSprite {
                                        index: grid_tile.t as usize,
                                        ..Default::default()
                                    },
                                    transform,
                                    ..Default::default()
                                });
                            }
                            // spawn collision
                            if let Some(collisions) = collisions {
                                for (collision, geometry) in collisions {
                                    parent
                                        .spawn_bundle(geometry)
                                        .insert(DebugTarget)
                                        .insert(Visibility { is_visible: false });
                                    parent
                                        .spawn_bundle(collision)
                                        .insert(ColliderPositionSync::Discrete);
                                }
                            }
                        });
                }
                _ => {
                    todo!("not implemented");
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct LdtkLoader;

impl AssetLoader for LdtkLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<(), anyhow::Error>> {
        Box::pin(async move {
            let data = serde_json::from_slice::<LdtkData>(bytes)?;
            let ldtk = Ldtk {
                data,
                file_path: load_context.path().to_path_buf(),
            };
            load_context.set_default_asset(LoadedAsset::new(ldtk));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ldtk"]
    }
}
fn on_asset_event_system(
    mut event_asset: EventReader<AssetEvent<Ldtk>>,
    asset_server: Res<AssetServer>,
    mut ldtks: ResMut<Assets<Ldtk>>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
    mut commands: Commands,
    rapier_config: Res<RapierConfiguration>,
    mut event_writer: EventWriter<LdtkEvent>,
) {
    for event in event_asset.iter() {
        match event {
            AssetEvent::Created { handle } => {
                if let Some(ldtk) = ldtks.get_mut(handle) {
                    for level_name in ["Level_0"] {
                        ldtk.load(
                            &level_name,
                            &asset_server,
                            &mut texture_atlases,
                            &mut commands,
                            &rapier_config,
                            &mut event_writer,
                        )
                        .unwrap();
                    }
                }
            }
            _ => {}
        }
    }
}

fn merge_polygons(polygons: &Vec<Vec<Vec2>>) -> Option<Vec<Vec<Vec2>>> {
    polygons
        .iter()
        .map(|polygon| {
            geo::MultiPolygon(vec![geo::Polygon::new(
                geo::LineString::from(
                    polygon
                        .iter()
                        .map(|v| geo::Coordinate {
                            x: v.x as f64,
                            y: v.y as f64,
                        })
                        .collect::<Vec<_>>(),
                ),
                vec![],
            )])
        })
        .reduce(|acc, polygon| acc.union(&polygon))
        .map(|multi_polygon| {
            multi_polygon
                .0
                .iter()
                .map(|polygon| {
                    let exterior = polygon.exterior();
                    return exterior
                        .points()
                        .map(|p| Vec2::new(p.x() as f32, p.y() as f32))
                        .collect::<Vec<_>>();
                })
                .collect::<Vec<_>>()
        })
}
