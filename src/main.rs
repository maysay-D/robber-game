use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use rand::{seq::SliceRandom, thread_rng};
use std::cmp::Reverse;

mod game_logic;
mod game_types;

use game_logic::{GameBoard, Position};
use game_types::{CpuDifficulty, PlayerTurn};

const CELL_SIZE: f32 = 72.0;
const TURN_SECONDS: f32 = 30.0;
const CPU_THINK_SECONDS: f32 = 0.6;

const BOARD_SIZES: [(usize, usize); 4] = [
    (3, 3),
    (4, 3),
    (4, 4),
    (5, 4),
];

fn player_color(player: PlayerTurn) -> Color {
    match player {
        PlayerTurn::First => Color::srgb(0.2, 0.55, 1.0),
        PlayerTurn::Second => Color::srgb(1.0, 0.25, 0.25),
    }
}

#[derive(States, Debug, Hash, Eq, PartialEq, Clone, Default)]
enum AppState {
    #[default]
    Title,
    Playing,
}

#[derive(Resource)]
struct TitleSettings {
    board_width: usize,
    board_height: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameMode {
    Cpu(CpuDifficulty),
    LocalPlayers,
}

#[derive(Resource)]
struct SelectedGameMode(GameMode);

#[derive(Resource)]
struct TitleMenuSelection(usize);

#[derive(Resource)]
struct LocalPlayer {
    side: Option<PlayerTurn>,
}

#[derive(Resource)]
struct TurnTimer {
    remaining_time: f32,
}

#[derive(Resource)]
struct Gameplay {
    board: GameBoard,
    turn: PlayerTurn,
    selected: Option<Position>,
    winner: Option<PlayerTurn>,
    last_cpu_move: Option<(Position, Position)>,
}

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct TitleEntity;

#[derive(Component)]
struct GameEntity;

#[derive(Component)]
struct TurnHighlight;

#[derive(Component)]
struct TimerText;

#[derive(Component)]
struct PlayerInfoText;

#[derive(Component)]
struct BoardSizeText;

#[derive(Component)]
struct RematchPrompt;

#[derive(Component)]
struct TitleMenuOption {
    index: usize,
}

#[derive(Component)]
struct Cell {
    position: Position,
}

#[derive(Component)]
struct StoneVisual {
    position: Position,
}

#[derive(Component)]
struct StoneCountText {
    position: Position,
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.08)))
        .insert_resource(TitleSettings {
            board_width: 4,
            board_height: 3,
        })
        .insert_resource(TitleMenuSelection(0))
        .add_plugins(game_plugins())
        .init_state::<AppState>()
        .add_systems(Startup, setup_camera)
        .add_systems(
            Update,
            web_controls
                .before(title_input_system)
                .before(rematch_prompt_keyboard_shortcuts)
                .before(playing_keyboard_shortcuts),
        )
        .add_systems(
            Update,
            (
                title_input_system,
                update_title_menu_visuals,
                update_title_window,
                update_board_size_text,
            )
                .chain()
                .run_if(in_state(AppState::Title)),
        )
        .add_systems(OnEnter(AppState::Title), enter_title)
        .add_systems(OnExit(AppState::Title), cleanup_title)
        .add_systems(OnEnter(AppState::Playing), setup_game)
        .add_systems(
            Update,
            (
                tick_turn_timer,
                handle_play_clicks,
                cpu_take_turn,
                rematch_prompt_keyboard_shortcuts,
                sync_rematch_prompt,
                playing_keyboard_shortcuts,
                update_board_visuals,
                update_turn_highlight,
                update_game_window_title,
                update_turn_timer,
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(OnExit(AppState::Playing), cleanup_game)
        .run();
}

fn game_plugins() -> impl PluginGroup {
    let window = Window {
        title: "Stomper".to_string(),
        resolution: (1100, 720).into(),
        ..default()
    };

    #[cfg(target_arch = "wasm32")]
    let window = Window {
        canvas: Some("#stomper-canvas".to_string()),
        fit_canvas_to_parent: true,
        prevent_default_event_handling: true,
        ..window
    };

    DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window),
        ..default()
    })
}

fn setup_camera(mut commands: Commands) {
    #[cfg(not(target_arch = "wasm32"))]
    commands.spawn((Camera2d, MainCamera));

    #[cfg(target_arch = "wasm32")]
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::AutoMin {
                min_width: 900.0,
                min_height: 600.0,
            },
            ..OrthographicProjection::default_2d()
        }),
        Msaa::Off,
        MainCamera,
    ));
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WEB_KEYS: std::cell::RefCell<Vec<KeyCode>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn web_action(action: &str) {
    if let Some(key) = web_action_key(action) {
        WEB_KEYS.with(|keys| {
            let mut keys = keys.borrow_mut();

            if keys.len() < 32 {
                keys.push(key);
            }
        });
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn web_action_key(action: &str) -> Option<KeyCode> {
    Some(match action {
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        "4" => KeyCode::Digit4,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        "enter" => KeyCode::Enter,
        "rematch" => KeyCode::KeyC,
        "title" => KeyCode::Escape,
        _ => return None,
    })
}

#[cfg(target_arch = "wasm32")]
fn web_controls(mut keyboard: ResMut<ButtonInput<KeyCode>>) {
    WEB_KEYS.with(|keys| {
        for key in keys.borrow_mut().drain(..) {
            keyboard.press(key);
            keyboard.release(key);
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn web_controls() {}

fn japanese_font(asset_server: &AssetServer) -> FontSource {
    FontSource::Handle(
        asset_server.load("fonts/NotoSansJP-Variable.ttf"),
    )
}

fn enter_title(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        Text2d::new(""),
        TextFont {
            font: japanese_font(&asset_server),
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(0.0, -215.0, 10.0),
        BoardSizeText,
        TitleEntity,
    ));

    commands.spawn((
        Sprite::from_color(
            Color::srgb(0.12, 0.12, 0.2),
            Vec2::new(5000.0, 5000.0),
        ),
        Transform::from_xyz(0.0, 0.0, -50.0),
        TitleEntity,
    ));

    commands.spawn((
        Text2d::new("Stomper"),
        TextFont {
            font: japanese_font(&asset_server),
            font_size: FontSize::Px(64.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(0.0, 180.0, 10.0),
        TitleEntity,
    ));

    commands.spawn((
        Text2d::new(
            "1/2/3/4 または ↑↓ で選択し、Enter で決定",
        ),
        TextFont {
            font: japanese_font(&asset_server),
            font_size: FontSize::Px(26.0),
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.85, 0.85)),
        Transform::from_xyz(0.0, 125.0, 10.0),
        TitleEntity,
    ));

    for (index, label) in title_menu_labels().into_iter().enumerate() {
        commands.spawn((
            Text2d::new(label),
            TextFont {
                font: japanese_font(&asset_server),
                font_size: FontSize::Px(34.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Transform::from_xyz(
                0.0,
                40.0 - index as f32 * 45.0,
                10.0,
            ),
            TitleMenuOption { index },
            TitleEntity,
        ));
    }

    commands.spawn((
        Text2d::new(
            "CPU戦はあなたが先手 / 対人戦は同じ端末で交互に操作",
        ),
        TextFont {
            font: japanese_font(&asset_server),
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgb(0.8, 0.8, 0.8)),
        Transform::from_xyz(0.0, -160.0, 10.0),
        TitleEntity,
    ));
}

fn cleanup_title(
    mut commands: Commands,
    query: Query<Entity, With<TitleEntity>>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn title_menu_labels() -> [&'static str; 4] {
    [
        "CPU（弱）と対戦",
        "CPU（中）と対戦",
        "CPU（強）と対戦",
        "プレイヤー同士で対戦",
    ]
}

fn title_input_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<TitleMenuSelection>,
    mut settings: ResMut<TitleSettings>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::ArrowUp)
        || keyboard.just_pressed(KeyCode::KeyW)
    {
        selection.0 =
            (selection.0 + title_menu_labels().len() - 1)
                % title_menu_labels().len();
    }

    if keyboard.just_pressed(KeyCode::ArrowDown)
        || keyboard.just_pressed(KeyCode::KeyS)
    {
        selection.0 =
            (selection.0 + 1) % title_menu_labels().len();
    }

    for (index, key) in [
        (0, KeyCode::Digit1),
        (1, KeyCode::Digit2),
        (2, KeyCode::Digit3),
        (3, KeyCode::Digit4),
    ] {
        if keyboard.just_pressed(key) {
            selection.0 = index;
        }
    }

    if keyboard.just_pressed(KeyCode::ArrowLeft)
        || keyboard.just_pressed(KeyCode::ArrowRight)
    {
        let index = BOARD_SIZES
            .iter()
            .position(|&size| {
                size == (settings.board_width, settings.board_height)
            })
            .unwrap_or(1);

        let step =
            if keyboard.just_pressed(KeyCode::ArrowRight) {
                1
            } else {
                BOARD_SIZES.len() - 1
            };

        let (width, height) =
            BOARD_SIZES[(index + step) % BOARD_SIZES.len()];

        settings.board_width = width;
        settings.board_height = height;
    }

    if keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::Space)
    {
        match selection.0 {
            0 => {
                commands.insert_resource(SelectedGameMode(
                    GameMode::Cpu(CpuDifficulty::Weak),
                ));
                commands.insert_resource(LocalPlayer {
                    side: Some(PlayerTurn::First),
                });
                next_state.set(AppState::Playing);
            }
            1 => {
                commands.insert_resource(SelectedGameMode(
                    GameMode::Cpu(CpuDifficulty::Medium),
                ));
                commands.insert_resource(LocalPlayer {
                    side: Some(PlayerTurn::First),
                });
                next_state.set(AppState::Playing);
            }
            2 => {
                commands.insert_resource(SelectedGameMode(
                    GameMode::Cpu(CpuDifficulty::Strong),
                ));
                commands.insert_resource(LocalPlayer {
                    side: Some(PlayerTurn::First),
                });
                next_state.set(AppState::Playing);
            }
            3 => {
                commands.insert_resource(SelectedGameMode(
                    GameMode::LocalPlayers,
                ));
                commands.insert_resource(LocalPlayer {
                    side: None,
                });
                next_state.set(AppState::Playing);
            }
            _ => {}
        }
    }
}

fn update_title_menu_visuals(
    selection: Res<TitleMenuSelection>,
    mut query: Query<(&TitleMenuOption, &mut TextColor)>,
) {
    for (option, mut color) in &mut query {
        *color = if option.index == selection.0 {
            TextColor(Color::srgb(1.0, 0.92, 0.3))
        } else {
            TextColor(Color::WHITE)
        };
    }
}

fn update_title_window(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    window.title =
        "Stomper (1/2/3:CPU 4:Player vs Player)".to_string();
}

fn update_board_size_text(
    settings: Res<TitleSettings>,
    mut query: Query<&mut Text2d, With<BoardSizeText>>,
) {
    for mut text in &mut query {
        text.0 = format!(
            "← → 盤面: {} × {}（3×3 ～ 5×4）",
            settings.board_width,
            settings.board_height,
        );
    }
}

fn setup_game(
    mut commands: Commands,
    settings: Res<TitleSettings>,
    local_player: Res<LocalPlayer>,
    mode: Res<SelectedGameMode>,
    asset_server: Res<AssetServer>,
) {
    let board = GameBoard::new_filled(
        settings.board_width,
        settings.board_height,
    );

    let board_width = board.width();
    let board_height = board.height();
    let board_pixel_width = board_extent(board_width);
    let board_pixel_height = board_extent(board_height);

    commands.insert_resource(Gameplay {
        board,
        turn: PlayerTurn::First,
        selected: None,
        winner: None,
        last_cpu_move: None,
    });

    commands.insert_resource(TurnTimer {
        remaining_time: TURN_SECONDS,
    });

    commands.spawn((
        Sprite::from_color(
            Color::srgba(0.2, 0.5, 1.0, 0.14),
            Vec2::new(5000.0, 5000.0),
        ),
        Transform::from_xyz(0.0, 0.0, -40.0),
        TurnHighlight,
        GameEntity,
    ));

    for y in 0..board_height {
        for x in 0..board_width {
            let p = Position { x, y };

            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.23, 0.23, 0.23),
                    Vec2::splat(CELL_SIZE - 3.0),
                ),
                Transform::from_translation(
                    board_to_world(
                        p,
                        board_width,
                        board_height,
                        -10.0,
                    ),
                ),
                Cell { position: p },
                GameEntity,
            ));

            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.08, 0.08, 0.08),
                    Vec2::splat(CELL_SIZE * 0.56),
                ),
                Transform::from_translation(
                    board_to_world(
                        p,
                        board_width,
                        board_height,
                        1.0,
                    ),
                ),
                StoneVisual { position: p },
                GameEntity,
            ));

            commands.spawn((
                Text2d::new("1"),
                TextFont {
                    font: japanese_font(&asset_server),
                    font_size: FontSize::Px(CELL_SIZE * 0.32),
                    ..default()
                },
                TextColor(Color::srgb(0.93, 0.93, 0.93)),
                Transform::from_translation(
                    board_to_world(
                        p,
                        board_width,
                        board_height,
                        3.0,
                    ),
                ),
                StoneCountText { position: p },
                GameEntity,
            ));
        }
    }

    commands.spawn((
        Text2d::new("Time: 30.0"),
        TextFont {
            font: japanese_font(&asset_server),
            font_size: FontSize::Px(40.0),
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.3, 0.3)),
        Transform::from_xyz(
            0.0,
            board_pixel_height / 2.0 + 80.0,
            10.0,
        ),
        TimerText,
        GameEntity,
    ));

    let my_color_text = match mode.0 {
        GameMode::Cpu(difficulty) => {
            format!(
                "あなた: 先手 / CPU: {}",
                difficulty.label(),
            )
        }
        GameMode::LocalPlayers => {
            "先手: プレイヤー1 / 後手: プレイヤー2"
                .to_string()
        }
    };

    let my_player_color = match local_player.side {
        Some(PlayerTurn::First) => {
            Color::srgb(0.2, 0.55, 1.0)
        }
        Some(PlayerTurn::Second) => {
            Color::srgb(1.0, 0.25, 0.25)
        }
        None => Color::WHITE,
    };

    commands.spawn((
        Text2d::new(my_color_text),
        TextFont {
            font: japanese_font(&asset_server),
            font_size: FontSize::Px(30.0),
            ..default()
        },
        TextColor(my_player_color),
        Transform::from_xyz(
            0.0,
            -board_pixel_height / 2.0 - 80.0,
            10.0,
        ),
        PlayerInfoText,
        GameEntity,
    ));

    commands.spawn((
        Sprite::from_color(
            Color::srgba(1.0, 1.0, 1.0, 0.06),
            Vec2::new(
                board_pixel_width + CELL_SIZE,
                board_pixel_height + CELL_SIZE,
            ),
        ),
        Transform::from_xyz(0.0, 0.0, -20.0),
        GameEntity,
    ));
}

fn cleanup_game(
    mut commands: Commands,
    entities: Query<Entity, With<GameEntity>>,
    gameplay: Option<Res<Gameplay>>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }

    if gameplay.is_some() {
        commands.remove_resource::<Gameplay>();
    }

    commands.remove_resource::<TurnTimer>();
    commands.remove_resource::<LocalPlayer>();
    commands.remove_resource::<SelectedGameMode>();
}

fn handle_play_clicks(
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera_q: Single<
        (&Camera, &GlobalTransform),
        With<MainCamera>,
    >,
    mut gameplay: ResMut<Gameplay>,
    local_player: Res<LocalPlayer>,
    mut turn_timer: ResMut<TurnTimer>,
) {
    if (!mouse.just_pressed(MouseButton::Left)
        && !touches.any_just_pressed())
        || gameplay.winner.is_some()
    {
        return;
    }

    if let Some(my_side) = local_player.side {
        if gameplay.turn != my_side {
            return;
        }
    }

    let cursor = touches
        .iter_just_pressed()
        .next()
        .map(|touch| touch.position())
        .or_else(|| window.cursor_position());

    let Some(cursor) = cursor else {
        return;
    };

    let (camera, camera_transform) = *camera_q;

    let Some(world_position) = camera
        .viewport_to_world_2d(camera_transform, cursor)
        .ok()
    else {
        return;
    };

    let Some(clicked_cell) = world_to_board(
        world_position,
        gameplay.board.width(),
        gameplay.board.height(),
    ) else {
        gameplay.selected = None;
        return;
    };

    if let Some(selected) = gameplay.selected {
        if selected == clicked_cell {
            gameplay.selected = None;
            return;
        }

        if gameplay.board.apply_move(selected, clicked_cell) {
            gameplay.last_cpu_move = None;

            let current = gameplay.turn;

            gameplay.turn = gameplay.turn.next();
            turn_timer.remaining_time = TURN_SECONDS;
            gameplay.selected = None;

            if !gameplay.board.has_legal_move() {
                gameplay.winner = Some(current);
            }

            return;
        }
    }

    if gameplay.board.stack_at(clicked_cell) > 0 {
        gameplay.selected = Some(clicked_cell);
    } else {
        gameplay.selected = None;
    }
}

fn cpu_take_turn(
    mut gameplay: ResMut<Gameplay>,
    mode: Res<SelectedGameMode>,
    local_player: Res<LocalPlayer>,
    mut turn_timer: ResMut<TurnTimer>,
) {
    let GameMode::Cpu(difficulty) = mode.0 else {
        return;
    };

    let Some(human_side) = local_player.side else {
        return;
    };

    let cpu_side = human_side.next();

    if gameplay.winner.is_some()
        || gameplay.turn != cpu_side
    {
        return;
    }

    // 人間の着手を表示するため、CPUは少し待つ。
    // 待機時間もCPU自身の制限時間に含める。
    if turn_timer.remaining_time
        > TURN_SECONDS - CPU_THINK_SECONDS
    {
        return;
    }

    let Some((from, to)) =
        choose_cpu_move(&gameplay.board, difficulty)
    else {
        gameplay.winner = Some(human_side);
        return;
    };

    if gameplay.board.apply_move(from, to) {
        gameplay.last_cpu_move = Some((from, to));

        let current = gameplay.turn;

        gameplay.turn = gameplay.turn.next();
        turn_timer.remaining_time = TURN_SECONDS;
        gameplay.selected = None;

        if !gameplay.board.has_legal_move() {
            gameplay.winner = Some(current);
        }
    }
}

fn choose_cpu_move(
    board: &GameBoard,
    difficulty: CpuDifficulty,
) -> Option<(Position, Position)> {
    let moves = board.legal_moves();

    if moves.is_empty() {
        return None;
    }

    let mut rng = thread_rng();

    match difficulty {
        CpuDifficulty::Weak => {
            moves.choose(&mut rng).copied()
        }
        CpuDifficulty::Medium => {
            let scored = moves
                .into_iter()
                .map(|(from, to)| {
                    ((from, to), score_move(board, from, to))
                })
                .collect::<Vec<_>>();

            let min_opponent = scored
                .iter()
                .map(|(_, score)| score.opponent_moves)
                .min()?;

            let mut best = scored
                .into_iter()
                .filter(|(_, score)| {
                    score.opponent_moves == min_opponent
                })
                .collect::<Vec<_>>();

            best.shuffle(&mut rng);

            best.first().map(|(mv, _)| *mv)
        }
        CpuDifficulty::Strong => {
            moves.into_iter().max_by_key(|(from, to)| {
                let score = score_move(board, *from, *to);

                (
                    Reverse(score.opponent_moves),
                    score.resulting_stack,
                    score.moving_stack,
                )
            })
        }
    }
}

#[derive(Clone, Copy)]
struct MoveScore {
    opponent_moves: usize,
    resulting_stack: u32,
    moving_stack: u32,
}

fn score_move(
    board: &GameBoard,
    from: Position,
    to: Position,
) -> MoveScore {
    let moving_stack = board.stack_at(from);
    let resulting_stack =
        moving_stack + board.stack_at(to);

    let mut next_board = board.clone();
    let _ = next_board.apply_move(from, to);

    MoveScore {
        opponent_moves: next_board.legal_moves().len(),
        resulting_stack,
        moving_stack,
    }
}

fn rematch_prompt_keyboard_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut gameplay: ResMut<Gameplay>,
    mut next_state: ResMut<NextState<AppState>>,
    mut turn_timer: ResMut<TurnTimer>,
) {
    if gameplay.winner.is_none() {
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyC)
        || keyboard.just_pressed(KeyCode::Enter)
    {
        reset_match(&mut gameplay, &mut turn_timer);
    } else if keyboard.just_pressed(KeyCode::KeyQ) {
        next_state.set(AppState::Title);
    }
}

fn sync_rematch_prompt(
    mut commands: Commands,
    gameplay: Option<Res<Gameplay>>,
    timer: Res<TurnTimer>,
    prompt_entities: Query<Entity, With<RematchPrompt>>,
    asset_server: Res<AssetServer>,
) {
    let should_show = matches!(
        gameplay.as_deref(),
        Some(gameplay) if gameplay.winner.is_some()
    );

    if should_show {
        if prompt_entities.is_empty() {
            let gameplay = gameplay.as_ref().unwrap();
            let winner = gameplay.winner.unwrap();

            let board_pixel_height =
                board_extent(gameplay.board.height());

            let status =
                "C / Enter: 再戦  Q / Esc: タイトル";

            commands.spawn((
                Sprite::from_color(
                    Color::srgba(0.0, 0.0, 0.0, 0.55),
                    Vec2::new(700.0, 170.0),
                ),
                Transform::from_xyz(
                    0.0,
                    board_pixel_height / 2.0 + 20.0,
                    20.0,
                ),
                RematchPrompt,
                GameEntity,
            ));

            commands.spawn((
                Text2d::new(format!(
                    "{}：{}の勝ち",
                    if timer.remaining_time > 0.0 {
                        "対局終了"
                    } else {
                        "時間切れ"
                    },
                    winner.label(),
                )),
                TextFont {
                    font: japanese_font(&asset_server),
                    font_size: FontSize::Px(34.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_xyz(
                    0.0,
                    board_pixel_height / 2.0 + 70.0,
                    25.0,
                ),
                RematchPrompt,
                GameEntity,
            ));

            commands.spawn((
                Text2d::new(status),
                TextFont {
                    font: japanese_font(&asset_server),
                    font_size: FontSize::Px(28.0),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.9, 0.35)),
                Transform::from_xyz(
                    0.0,
                    board_pixel_height / 2.0 + 20.0,
                    25.0,
                ),
                RematchPrompt,
                GameEntity,
            ));
        }
    } else {
        for entity in &prompt_entities {
            commands.entity(entity).despawn();
        }
    }
}

fn playing_keyboard_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::KeyR)
        || keyboard.just_pressed(KeyCode::Escape)
    {
        next_state.set(AppState::Title);
    }
}

fn update_board_visuals(
    gameplay: Res<Gameplay>,
    mut cells: Query<
        (&Cell, &mut Sprite),
        Without<StoneVisual>,
    >,
    mut stones: Query<
        (&StoneVisual, &mut Sprite, &mut Transform),
        Without<Cell>,
    >,
    mut stone_texts: Query<
        (&StoneCountText, &mut Text2d),
        Without<Cell>,
    >,
) {
    for (cell, mut sprite) in &mut cells {
        let mut color = Color::srgb(0.23, 0.23, 0.23);

        if let Some((from, to)) = gameplay.last_cpu_move {
            if cell.position == from {
                color = Color::srgb(0.55, 0.36, 0.1);
            } else if cell.position == to {
                color = Color::srgb(0.85, 0.6, 0.12);
            }
        }

        if gameplay.selected == Some(cell.position) {
            color = player_color(gameplay.turn);
        }

        sprite.color = color;
    }

    for (stone, mut sprite, mut transform) in &mut stones {
        let height =
            gameplay.board.stack_at(stone.position);

        if height == 0 {
            sprite.color =
                Color::srgba(0.0, 0.0, 0.0, 0.0);
            transform.scale = Vec3::ONE;
            continue;
        }

        sprite.color = Color::srgb(0.04, 0.04, 0.04);

        let scale =
            0.45 + (height as f32).sqrt() * 0.2;

        transform.scale =
            Vec3::splat(scale.min(1.7));
    }

    for (stone_text, mut text) in &mut stone_texts {
        let height =
            gameplay.board.stack_at(stone_text.position);

        text.0 = if height == 0 {
            String::new()
        } else {
            height.to_string()
        };
    }
}

fn update_turn_highlight(
    gameplay: Res<Gameplay>,
    mut query: Single<&mut Sprite, With<TurnHighlight>>,
) {
    query.color = match gameplay.turn {
        PlayerTurn::First => {
            Color::srgba(0.2, 0.55, 1.0, 0.14)
        }
        PlayerTurn::Second => {
            Color::srgba(1.0, 0.25, 0.25, 0.14)
        }
    };
}

fn update_turn_timer(
    timer_res: Res<TurnTimer>,
    mut timer_text_query: Query<
        (&mut Text2d, &mut TextColor, &mut Visibility),
        With<TimerText>,
    >,
    gameplay: Res<Gameplay>,
) {
    for (mut text, mut color, mut visibility)
    in &mut timer_text_query
    {
        // 終局時の勝敗は結果パネルだけに表示する。
        *visibility = if gameplay.winner.is_some() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };

        text.0 = format!(
            "{}の手番  Time: {}",
            gameplay.turn.label(),
            remaining_time_label(timer_res.remaining_time),
        );

        *color =
            TextColor(player_color(gameplay.turn));
    }
}

fn remaining_time_label(seconds: f32) -> String {
    // 残り時間が正の場合に0.0と表示しない。
    format!(
        "{:.1}",
        (seconds.max(0.0) * 10.0).ceil() / 10.0,
    )
}

fn update_game_window_title(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    gameplay: Res<Gameplay>,
    mode: Res<SelectedGameMode>,
) {
    let mode_str = match mode.0 {
        GameMode::Cpu(difficulty) => {
            format!("[{}]", difficulty.label())
        }
        GameMode::LocalPlayers => {
            "[プレイヤー同士の対戦]".to_string()
        }
    };

    if let Some(winner) = gameplay.winner {
        window.title = format!(
            "Stomper Game {} | {} の勝ち",
            mode_str,
            winner.label(),
        );
    } else {
        window.title = format!(
            "Stomper Game {} | {} の手番 (盤面 {}x{})",
            mode_str,
            gameplay.turn.label(),
            gameplay.board.width(),
            gameplay.board.height(),
        );
    }
}

fn board_extent(size: usize) -> f32 {
    size as f32 * CELL_SIZE
}

fn board_to_world(
    pos: Position,
    width: usize,
    height: usize,
    z: f32,
) -> Vec3 {
    let half_width = board_extent(width) / 2.0;
    let half_height = board_extent(height) / 2.0;

    let x =
        (pos.x as f32 + 0.5) * CELL_SIZE - half_width;
    let y =
        (pos.y as f32 + 0.5) * CELL_SIZE - half_height;

    Vec3::new(x, y, z)
}

fn world_to_board(
    world: Vec2,
    width: usize,
    height: usize,
) -> Option<Position> {
    let half_width = board_extent(width) / 2.0;
    let half_height = board_extent(height) / 2.0;

    let x =
        ((world.x + half_width) / CELL_SIZE).floor() as isize;
    let y =
        ((world.y + half_height) / CELL_SIZE).floor() as isize;

    if x < 0
        || y < 0
        || x >= width as isize
        || y >= height as isize
    {
        return None;
    }

    Some(Position {
        x: x as usize,
        y: y as usize,
    })
}

fn reset_match(
    gameplay: &mut Gameplay,
    turn_timer: &mut TurnTimer,
) {
    let width = gameplay.board.width();
    let height = gameplay.board.height();

    gameplay.board =
        GameBoard::new_filled(width, height);
    gameplay.turn = PlayerTurn::First;
    gameplay.selected = None;
    gameplay.winner = None;
    gameplay.last_cpu_move = None;

    turn_timer.remaining_time = TURN_SECONDS;
}

fn tick_turn_timer(
    time: Res<Time<Real>>,
    mut timer: ResMut<TurnTimer>,
    mut gameplay: ResMut<Gameplay>,
) {
    advance_turn_timer(
        &mut gameplay,
        &mut timer,
        time.delta_secs(),
    );
}

fn advance_turn_timer(
    gameplay: &mut Gameplay,
    timer: &mut TurnTimer,
    delta: f32,
) {
    if gameplay.winner.is_some() {
        return;
    }

    timer.remaining_time =
        (timer.remaining_time - delta).max(0.0);

    if timer.remaining_time == 0.0 {
        gameplay.winner =
            Some(gameplay.turn.next());
        gameplay.selected = None;
    }
}