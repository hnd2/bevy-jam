use bevy::prelude::*;

#[derive(Component)]
pub struct DebugTarget;

pub struct DebugPlugin;
impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(debug_system)
            .add_system(bevy::input::system::exit_on_esc_system);
    }
}

fn debug_system(
    mut query: Query<&mut Visibility, With<DebugTarget>>,
    keyboard_input: Res<Input<KeyCode>>,
) {
    if keyboard_input.just_pressed(KeyCode::Key1) {
        for mut visibility in query.iter_mut() {
            visibility.is_visible = true;
        }
    }
    if keyboard_input.just_pressed(KeyCode::Key2) {
        for mut visibility in query.iter_mut() {
            visibility.is_visible = false;
        }
    }
}
