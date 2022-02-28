mod data;

use self::data::AsepriteData;
use anyhow::{anyhow, Context, Result};
use bevy::{
    asset::{AssetLoader, LoadContext, LoadedAsset},
    prelude::*,
    reflect::TypeUuid,
    utils::BoxedFuture,
};
use regex::Regex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct AsepritePlugin;
impl Plugin for AsepritePlugin {
    fn build(&self, app: &mut App) {
        app.add_asset::<Aseprite>()
            .init_asset_loader::<AsepriteLoader>()
            .add_system(animation_sprite_system)
            .add_system(on_asset_event_system);
    }
}

#[derive(Debug)]
pub struct AnimationFrame {
    pub index: usize,
    pub duration: f32,
    // pub collision_rect: Option<Rect>,
}
#[derive(Debug)]
pub struct Animation {
    pub name: String,
    pub frames: Vec<AnimationFrame>,
}

#[derive(Component)]
pub struct AnimationSprite {
    pub aseprite: Handle<Aseprite>,
    timer: Timer,
    current_animation_name: String,
    current_frame_index: usize,
    loop_animation: bool,
    is_dirty: bool,
    speed: f32,
    //paused
}

impl AnimationSprite {
    pub fn new(aseprite: Handle<Aseprite>) -> Self {
        Self {
            aseprite,
            timer: Timer::new(Duration::from_secs(0), false),
            current_animation_name: "".to_string(),
            current_frame_index: 0,
            loop_animation: true,
            is_dirty: true,
            speed: 2.0,
        }
    }
    pub fn set_animation(&mut self, name: &str, loop_animation: bool) {
        if self.current_animation_name == name {
            return;
        }
        self.current_animation_name = name.to_owned();
        self.current_frame_index = 0;
        self.loop_animation = loop_animation;
        self.is_dirty = true;
    }
}

#[derive(Debug, TypeUuid)]
#[uuid = "e60607bc-972e-11ec-b909-0242ac120002"]
pub struct Aseprite {
    pub data: AsepriteData,
    pub file_path: PathBuf,
    pub rects: Vec<bevy::sprite::Rect>,
    pub animations: HashMap<String, Animation>,
}

impl Aseprite {
    pub fn new(file_path: &Path, data: AsepriteData) -> Self {
        // animations
        let frames = {
            let re = Regex::new(r".*\D(\d+).aseprite").expect("Failed to parse regex");
            let mut frames = data
                .frames
                .iter()
                .filter_map(|(key, value)| {
                    usize::from_str_radix(re.replace(key, "$1").as_ref(), 10)
                        .map(|index| (index, value))
                        .ok()
                })
                .collect::<Vec<_>>();
            frames.sort_by(|a, b| a.0.cmp(&b.0));
            frames
                .into_iter()
                .map(|(_, value)| value)
                .collect::<Vec<_>>()
        };
        let rects = frames
            .iter()
            .map(|self::data::FrameValue { frame, .. }| {
                let min = Vec2::new(frame.x as f32, frame.y as f32);
                let size = Vec2::new(frame.w as f32, frame.h as f32);
                bevy::sprite::Rect {
                    min,
                    max: min + size,
                }
            })
            .collect();
        let animations = data
            .meta
            .frame_tags
            .iter()
            .map(|tag| {
                let frames = (tag.from..=tag.to)
                    .into_iter()
                    .filter_map(|index| {
                        frames.get(index as usize).map(|frame| AnimationFrame {
                            index: index as usize,
                            duration: (frame.duration as f32) / 1000.0,
                        })
                    })
                    .collect();
                (
                    tag.name.to_owned(),
                    Animation {
                        name: tag.name.to_owned(),
                        frames,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        Self {
            data,
            file_path: file_path.to_path_buf(),
            rects,
            animations,
        }
    }
}

#[derive(Default)]
pub struct AsepriteLoader;
impl AssetLoader for AsepriteLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<(), anyhow::Error>> {
        Box::pin(async move {
            let data = serde_json::from_slice::<AsepriteData>(bytes)?;
            let aseprite = Aseprite::new(&load_context.path(), data);
            load_context.set_default_asset(LoadedAsset::new(aseprite));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["json"]
    }
}

fn create_texture_atlas(
    aseprite: &Aseprite,
    asset_server: &Res<AssetServer>,
) -> Result<TextureAtlas> {
    // create texture atlas
    let base_path = aseprite
        .file_path
        .parent()
        .with_context(|| format!("failed to get parent directory, {:?}", aseprite.file_path))?;
    let mut texture_path = std::path::PathBuf::new();
    texture_path.push(base_path);
    texture_path.push(&aseprite.data.meta.image);

    let texture_handle = asset_server.load(texture_path.as_path());
    let mut texture_atlas = TextureAtlas::new_empty(
        texture_handle,
        Vec2::new(
            aseprite.data.meta.size.w as f32,
            aseprite.data.meta.size.h as f32,
        ),
    );
    for rect in &aseprite.rects {
        texture_atlas.add_texture(rect.to_owned());
    }
    Ok(texture_atlas)
}
fn animation_sprite_system(
    time: Res<Time>,
    mut query: Query<(&mut AnimationSprite, &mut TextureAtlasSprite)>,
    aseprites: ResMut<Assets<Aseprite>>,
) {
    let set_new_frame = |sprite: &mut Mut<AnimationSprite>,
                         texture_atlas_sprite: &mut Mut<TextureAtlasSprite>,
                         animation: &Animation| {
        if let Some(frame) = animation.frames.get(sprite.current_frame_index) {
            let time = frame.duration / sprite.speed;
            sprite.timer.set_duration(Duration::from_secs_f32(time));
            sprite.timer.reset();
            texture_atlas_sprite.index = frame.index;
        }
    };
    for (mut sprite, mut texture_atlas_sprite) in query.iter_mut() {
        if let Some(aseprite) = aseprites.get(&sprite.aseprite) {
            // get animation frame
            if sprite.is_dirty {
                if let Some(animation) = aseprite.animations.get(&sprite.current_animation_name) {
                    set_new_frame(&mut sprite, &mut texture_atlas_sprite, animation);
                }
                sprite.is_dirty = false;
            } else {
                sprite.timer.tick(time.delta());
                if sprite.timer.just_finished() {
                    if let Some(animation) = aseprite.animations.get(&sprite.current_animation_name)
                    {
                        if sprite.current_frame_index + 1 > animation.frames.len() - 1 {
                            if sprite.loop_animation {
                                sprite.current_frame_index = 0;
                                set_new_frame(&mut sprite, &mut texture_atlas_sprite, animation);
                            } else {
                                // pause
                            }
                        } else {
                            sprite.current_frame_index += 1;
                            set_new_frame(&mut sprite, &mut texture_atlas_sprite, animation);
                        }
                    }
                }
            }
        }
    }
}

fn on_asset_event_system(
    mut event_asset: EventReader<AssetEvent<Aseprite>>,
    asset_server: Res<AssetServer>,
    aseprites: ResMut<Assets<Aseprite>>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut AnimationSprite)>,
) {
    for event in event_asset.iter() {
        match event {
            AssetEvent::Created { handle } => {
                let aseprite = aseprites.get(handle).unwrap();
                let texture_atlas_handle = create_texture_atlas(&aseprite, &asset_server)
                    .map(|texture_atlas| texture_atlases.add(texture_atlas))
                    .unwrap();

                for (entity, _) in query
                    .iter_mut()
                    .filter(|(_, sprite)| sprite.aseprite == *handle)
                {
                    commands
                        .entity(entity)
                        .remove::<Handle<TextureAtlas>>()
                        .insert(texture_atlas_handle.clone());
                }
            }
            _ => {}
        }
    }
}
