mod animation;
mod debug;
mod ldtk;
use animation::{AnimationSprite, Aseprite, AsepritePlugin};
use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use debug::*;
use ldtk::plugin::{Ldtk, LdtkEvent, LdtkPlugin};

fn main() {
    App::new()
        .insert_resource(WindowDescriptor {
            width: 320.0,
            height: 240.0,
            scale_factor_override: Some(2.0),
            resizable: false,
            ..Default::default()
        })
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(RapierPhysicsPlugin::<NoUserData>::default())
        //.add_plugin(RapierRenderPlugin)
        .add_plugin(ShapePlugin)
        .add_plugin(LdtkPlugin)
        .add_plugin(DebugPlugin)
        .add_plugin(AsepritePlugin)
        .add_startup_system(setup_system)
        .add_system(player_system)
        .add_system(camera_system)
        .add_system(on_collision_event_system)
        .add_system(on_ldtk_event_system)
        .run();
}

const RAPIER_SCALE: f32 = 32.0; // 1m = 32px
const Z_COLLISION: f32 = 10.0;

#[derive(PartialEq, Eq)]
enum Direction {
    Left,
    Right,
}

#[derive(Debug)]
enum PlayerState {
    Wait,
    Walk,
    Run,
    Attack,
    Guard,
}

#[derive(Component)]
struct Player {
    state: PlayerState,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            state: PlayerState::Wait,
        }
    }
}

#[derive(Component)]
struct Enemy;

#[derive(Component)]
struct Actor {
    direction: Direction,
}
impl Actor {
    fn new() -> Self {
        Self {
            direction: Direction::Right,
        }
    }
}

#[derive(Component)]
struct VirtualPosition(Vec3);

fn setup_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut rapier_config: ResMut<RapierConfiguration>,
) {
    rapier_config.scale = RAPIER_SCALE;

    // origin for debug
    commands
        .spawn_bundle(
            GeometryBuilder::new()
                .add(&shapes::Circle {
                    radius: 1.0,
                    center: Vec2::ZERO,
                })
                .build(
                    DrawMode::Fill(FillMode::color(Color::FUCHSIA)),
                    Transform::identity(),
                ),
        )
        .insert(DebugTarget)
        .insert(Visibility { is_visible: false });

    let scene: Handle<Ldtk> = asset_server.load("levels.ldtk");
    commands.insert_resource(scene);

    // camera
    commands
        .spawn_bundle(OrthographicCameraBundle::new_2d())
        .insert(VirtualPosition(Vec3::ZERO));
}
fn camera_system(
    mut cameras: Query<(&mut Transform, &mut VirtualPosition), (With<Camera>, Without<Player>)>,
    players: Query<&Transform, With<Player>>,
) {
    if cameras.is_empty() || players.is_empty() {
        return;
    }
    let (mut camera_transform, mut position) = cameras.single_mut();
    let player_transform = players.single();

    // lerp
    let ratio = 0.05;
    let mut x = position.0.x * (1.0 - ratio) + player_transform.translation.x * ratio;
    position.0.x = x;

    // align pixel
    //x = (x * 2.0).round() / 2.0;

    camera_transform.translation.x = x;
}

fn player_system(
    mut commands: Commands,
    mut players: Query<
        (
            &mut Actor,
            &Children,
            &RigidBodyPositionComponent,
            &mut RigidBodyVelocityComponent,
            &RigidBodyMassPropsComponent,
            &mut ColliderMaterialComponent,
        ),
        With<Player>,
    >,
    mut sprites: Query<(
        &mut Transform,
        &mut AnimationSprite,
        &mut TextureAtlasSprite,
    )>,
    enemies: Query<&Enemy>,
    keyboard_input: Res<Input<KeyCode>>,
    rapier_config: Res<RapierConfiguration>,
    query_pipeline: Res<QueryPipeline>,
    collider_query: QueryPipelineColliderComponentsQuery,
) {
    if players.is_empty() {
        return;
    }
    let (mut actor, children, rb_position, mut rb_velocity, rb_mass_props, mut collider_material) =
        players.single_mut();

    let left = keyboard_input.pressed(KeyCode::A) || keyboard_input.pressed(KeyCode::Left);
    let right = keyboard_input.pressed(KeyCode::D) || keyboard_input.pressed(KeyCode::Right);
    let x_axis = -(left as i8) + right as i8;
    let mut move_delta = Vec2::new(x_axis as f32, 0.0);
    if move_delta != Vec2::ZERO {
        move_delta /= move_delta.length() * rapier_config.scale;
        collider_material.friction = 0.0;
    } else {
        collider_material.friction = 1.0;
    }
    let jump = keyboard_input.just_pressed(KeyCode::Space);
    let attack = keyboard_input.just_pressed(KeyCode::Z);

    let hold = keyboard_input.pressed(KeyCode::LShift);
    if !hold && left {
        actor.direction = Direction::Left;
    } else if !hold && right {
        actor.direction = Direction::Right;
    }
    let flip_x = if actor.direction == Direction::Left {
        -1.0
    } else {
        1.0
    };

    rb_velocity.linvel.x = move_delta.x * 24.0;
    if jump {
        let force = Vec2::new(0.0, 8.0) / rapier_config.scale;
        rb_velocity.apply_impulse(&rb_mass_props, force.into());
    }
    if attack {
        let force = Vec2::new(32.0 * flip_x, 0.0) / rapier_config.scale;
        rb_velocity.apply_impulse(&rb_mass_props, force.into());

        let collider_set = QueryPipelineColliderComponentsSet(&collider_query);
        let shape = Cuboid::new((Vec2::new(16.0, 16.0) / RAPIER_SCALE).into());
        let mut shape_pos = (Vec2::new(16.0 * flip_x, 0.0) / RAPIER_SCALE).into();
        shape_pos *= rb_position.position;

        query_pipeline.intersections_with_shape(
            &collider_set,
            &shape_pos,
            &shape,
            InteractionGroups::all(),
            None,
            |handle| {
                let entity = handle.entity();
                if let Ok(enemy) = enemies.get(entity) {
                    commands.entity(entity).despawn_recursive();
                }
                true
            },
        );
    }

    // animate sprite
    if let Some((mut transform, mut animation_sprite, mut texture_atlas_sprite)) = children
        .iter()
        .next()
        .and_then(|child| sprites.get_mut(*child).ok())
    {
        if attack {
            animation_sprite.set_animation("attack", false);
        } else if x_axis != 0 {
            animation_sprite.set_animation("walk", true);
        } else {
            animation_sprite.set_animation("wait", false);
        }
        texture_atlas_sprite.flip_x = flip_x < 0.0;
        transform.translation.x = transform.translation.x.abs() * flip_x;
    }
}
fn on_collision_event_system(
    mut intersection_events: EventReader<IntersectionEvent>,
    mut contact_events: EventReader<ContactEvent>,
) {
    for event in intersection_events.iter() {
        println!("{:?}", event);
    }
    for event in contact_events.iter() {
        println!("{:?}", event);
    }
}

fn on_ldtk_event_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut cameras: Query<(&mut VirtualPosition, &mut Transform), With<Camera>>,
    mut events: EventReader<LdtkEvent>,
) {
    for event in events.iter() {
        match event {
            LdtkEvent::SpawnPlayer(position) => {
                let aseprite: Handle<Aseprite> = asset_server.load("images/character.json");

                // spawn player
                commands
                    .spawn()
                    .insert_bundle(RigidBodyBundle {
                        position: (position.truncate() / RAPIER_SCALE).into(),
                        mass_properties: RigidBodyMassPropsFlags::ROTATION_LOCKED.into(),
                        ..Default::default()
                    })
                    .insert_bundle(ColliderBundle {
                        shape: ColliderShape::capsule(
                            (Vec2::new(0.0, 6.0) / RAPIER_SCALE).into(),
                            (Vec2::new(0.0, -6.0) / RAPIER_SCALE).into(),
                            4.0 / RAPIER_SCALE,
                        )
                        .into(),
                        material: ColliderMaterial::new(1.0, 0.0).into(),
                        ..Default::default()
                    })
                    .insert(ColliderPositionSync::Discrete)
                    .insert(Actor::new())
                    .insert(Player::default())
                    .with_children(|parent| {
                        parent
                            .spawn_bundle(SpriteSheetBundle {
                                transform: Transform::from_xyz(4.0, 6.0, 0.0),
                                ..Default::default()
                            })
                            .insert(AnimationSprite::new(aseprite.clone()));

                        // debug collider
                        parent
                            .spawn_bundle(
                                GeometryBuilder::new()
                                    .add(&shapes::Circle {
                                        radius: 4.0,
                                        center: Vec2::new(0.0, 6.0),
                                    })
                                    .add(&shapes::Circle {
                                        radius: 4.0,
                                        center: Vec2::new(0.0, -6.0),
                                    })
                                    .add(&shapes::Rectangle {
                                        extents: Vec2::new(8.0, 12.0),
                                        origin: RectangleOrigin::Center,
                                    })
                                    .build(
                                        DrawMode::Fill(FillMode {
                                            options: FillOptions::non_zero(),
                                            color: Color::rgba(1.0, 0.0, 1.0, 0.2),
                                        }),
                                        Transform::from_xyz(0.0, 0.0, Z_COLLISION),
                                    ),
                            )
                            .insert(DebugTarget)
                            .insert(Visibility { is_visible: false });

                        // debug text
                        parent
                            .spawn_bundle(Text2dBundle {
                                text: Text::with_section(
                                    "player".to_string(),
                                    TextStyle {
                                        font: asset_server.load("fonts/hack.ttf"),
                                        font_size: 6.0,
                                        color: Color::rgb(1.0, 0.0, 1.0),
                                    },
                                    TextAlignment {
                                        horizontal: HorizontalAlign::Center,
                                        vertical: VerticalAlign::Center,
                                    },
                                ),
                                transform: Transform::from_xyz(0.0, 28.0, Z_COLLISION + 1.0),
                                ..Default::default()
                            })
                            .insert(DebugTarget)
                            .insert(Visibility { is_visible: false });

                        let (mut camera_position, mut camera_transform) = cameras.single_mut();
                        camera_position.0.x = position.x;
                        camera_transform.translation.x = position.x;
                    });
            }
            LdtkEvent::SpawnEnemy { name, position } if name == "test" => {
                let aseprite: Handle<Aseprite> = asset_server.load("images/character.json");
                // spawn player
                commands
                    .spawn()
                    .insert_bundle(RigidBodyBundle {
                        position: (position.truncate() / RAPIER_SCALE).into(),
                        mass_properties: RigidBodyMassPropsFlags::ROTATION_LOCKED.into(),
                        ..Default::default()
                    })
                    .insert_bundle(ColliderBundle {
                        shape: ColliderShape::capsule(
                            (Vec2::new(0.0, 6.0) / RAPIER_SCALE).into(),
                            (Vec2::new(0.0, -6.0) / RAPIER_SCALE).into(),
                            4.0 / RAPIER_SCALE,
                        )
                        .into(),
                        material: ColliderMaterial::new(1.0, 0.0).into(),
                        ..Default::default()
                    })
                    .insert(ColliderPositionSync::Discrete)
                    .insert(Actor::new())
                    .insert(Enemy)
                    .with_children(|parent| {
                        parent
                            .spawn_bundle(SpriteSheetBundle {
                                transform: Transform::from_xyz(4.0, 6.0, 0.0),
                                ..Default::default()
                            })
                            .insert(AnimationSprite::new(aseprite.clone()));
                        // collision debug
                        parent
                            .spawn_bundle(
                                GeometryBuilder::new()
                                    .add(&shapes::Circle {
                                        radius: 4.0,
                                        center: Vec2::new(0.0, 6.0),
                                    })
                                    .add(&shapes::Circle {
                                        radius: 4.0,
                                        center: Vec2::new(0.0, -6.0),
                                    })
                                    .add(&shapes::Rectangle {
                                        extents: Vec2::new(8.0, 12.0),
                                        origin: RectangleOrigin::Center,
                                    })
                                    .build(
                                        DrawMode::Fill(FillMode {
                                            options: FillOptions::non_zero(),
                                            color: Color::rgba(1.0, 0.0, 1.0, 0.2),
                                        }),
                                        Transform::from_xyz(0.0, 0.0, Z_COLLISION),
                                    ),
                            )
                            .insert(DebugTarget)
                            .insert(Visibility { is_visible: false });
                        parent
                            .spawn_bundle(Text2dBundle {
                                text: Text::with_section(
                                    name,
                                    TextStyle {
                                        font: asset_server.load("fonts/hack.ttf"),
                                        font_size: 6.0,
                                        color: Color::rgb(1.0, 0.0, 1.0),
                                    },
                                    TextAlignment {
                                        horizontal: HorizontalAlign::Center,
                                        vertical: VerticalAlign::Center,
                                    },
                                ),
                                transform: Transform::from_xyz(0.0, 28.0, Z_COLLISION + 1.0),
                                ..Default::default()
                            })
                            .insert(DebugTarget)
                            .insert(Visibility { is_visible: false });

                        let (mut camera_position, mut camera_transform) = cameras.single_mut();
                        camera_position.0.x = position.x;
                        camera_transform.translation.x = position.x;
                    });
            }
            _ => {}
        }
    }
}
