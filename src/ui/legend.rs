//! Color/shape legend panel, toggled with L key.

use bevy::prelude::*;

/// Resource tracking whether the legend is visible.
#[derive(Resource)]
pub struct LegendVisible(pub bool);

impl Default for LegendVisible {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component)]
pub struct LegendText;

pub fn setup_legend(mut commands: Commands) {
    commands.spawn((
        LegendText,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(14.0),
            top: Val::Px(14.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.06, 0.12, 0.82)),
    ));
}

pub fn toggle_legend(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<LegendVisible>) {
    if keys.just_pressed(KeyCode::KeyL) {
        visible.0 = !visible.0;
    }
}

pub fn update_legend(
    visible: Res<LegendVisible>,
    mut query: Query<(&mut Text, &mut Node), With<LegendText>>,
) {
    let Ok((mut text, mut node)) = query.single_mut() else {
        return;
    };

    if !visible.0 {
        node.display = Display::None;
        return;
    }
    node.display = Display::DEFAULT;

    let mut s = String::new();

    s.push_str("Ship Cargo Colors\n");
    s.push_str("  Grain    - Yellow\n");
    s.push_str("  Timber   - Brown\n");
    s.push_str("  Iron     - Grey\n");
    s.push_str("  Tools    - Blue\n");
    s.push_str("  Spices   - Red\n");
    s.push_str("  Empty    - White\n");

    s.push_str("\nShip Shapes\n");
    s.push_str("  Clipper   - Triangle\n");
    s.push_str("  Freighter - Rectangle\n");
    s.push_str("  Shorthaul - Small circle\n");

    s.push_str("\nIsland Colors\n");
    s.push_str("  Grain    - Sandy tan\n");
    s.push_str("  Timber   - Dark green\n");
    s.push_str("  Iron     - Grey-brown\n");
    s.push_str("  Spices   - Terracotta\n");
    s.push_str("  Tools    - Olive\n");

    s.push_str("\n[L] Toggle legend");

    **text = s;
}
