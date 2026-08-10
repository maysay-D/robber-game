use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use rand::{seq::SliceRandom, thread_rng};
use std::cmp::Reverse;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};
use std::process::{Command, Stdio};
use std::thread;

mod game_logic;
mod protocol;

use game_logic::{GameBoard, Position};
use protocol::{CpuDifficulty, GameMessage, PlayerTurn};

const CELL_SIZE: f32 = 72.0;

#[derive(States, Debug, Hash, Eq, PartialEq, Clone, Default)]
enum AppState {
    #[default]
    Title,
    Connecting,
    WaitingForMatch,
    Playing,
}

#[derive(Resource)]
struct ConnectionReceiver(Mutex<Receiver<TcpStream>>);

#[derive(Resource)]
struct TitleSettings {
    board_width: usize,
    board_height: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameMode {
    Cpu(CpuDifficulty),
    OnlinePlayer,
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
struct NetworkChannels {
    tx: Mutex<Sender<GameMessage>>,
    rx: Mutex<Receiver<GameMessage>>,
    stream: Mutex<TcpStream>,
}

#[derive(Resource)]
struct TurnTimer {
    remaining_time: f32,
}

#[derive(Component)]
struct TimerText;

#[derive(Component)]
struct PlayerInfoText;

// ----------------------------------------------------------------

#[derive(Resource)]
struct Gameplay {
    board: GameBoard,
    turn: PlayerTurn,
    selected: Option<Position>,
    winner: Option<PlayerTurn>,
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
            board_height: 1,
        })
        .insert_resource(TitleMenuSelection(0))
        .add_plugins(DefaultPlugins)
        .init_state::<AppState>()
        .add_systems(Startup, setup_camera)
        .add_systems(
            Update,
            (
                title_input_system,
                update_title_menu_visuals,
                update_title_window,
            )
                .run_if(in_state(AppState::Title)),
        )
        .add_systems(OnEnter(AppState::Title), enter_title)
        .add_systems(OnExit(AppState::Title), cleanup_title)
        .add_systems(OnEnter(AppState::Connecting), start_network_connection)
        .add_systems(
            Update,
            (
                check_network_connection,
                update_connecting_window,
                connecting_keyboard_shortcuts,
            ).run_if(in_state(AppState::Connecting))
        )
        .add_systems(
            Update,
            (
                wait_for_opponent,
                update_waiting_window,
                waiting_keyboard_shortcuts,
            ).run_if(in_state(AppState::WaitingForMatch))
        )
        .add_systems(OnEnter(AppState::Playing), setup_game)
        .add_systems(
            Update,
            (
                handle_play_clicks,
                handle_network_messages,
                cpu_take_turn,
                rematch_prompt_keyboard_shortcuts,
                sync_rematch_prompt,
                playing_keyboard_shortcuts,
                update_board_visuals,
                update_turn_highlight,
                update_game_window_title,
                update_turn_timer,
            )
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(OnExit(AppState::Playing), cleanup_game)
        .run();
}

#[cfg(not(target_arch = "wasm32"))]
fn start_network_connection(
    mut commands: Commands,
    mode_res: Res<SelectedGameMode>,
) {
    let (tx, rx) = mpsc::channel();

    match mode_res.0 {
        GameMode::OnlinePlayer => {
            println!("サーバー(172.192.161.164:8080)に接続中...");
            thread::spawn(move || {
                if let Ok(stream) = TcpStream::connect("172.192.161.164:8080") {
                    println!("サーバーに接続しました！");
                    let _ = tx.send(stream);
                }
            });
        }
        GameMode::Cpu(_) => unreachable!(),
    }

    commands.insert_resource(ConnectionReceiver(Mutex::new(rx)));
}

#[cfg(target_arch = "wasm32")]
fn start_network_connection(mut next_state: ResMut<NextState<AppState>>) {
    println!("WASM build does not support native TCP multiplayer.");
    next_state.set(AppState::Title);
}

fn check_network_connection(
    mut commands: Commands,
    receiver: Option<Res<ConnectionReceiver>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let Some(receiver) = receiver {
        if let Ok(stream) = receiver.0.lock().unwrap().try_recv() {
            setup_tcp_threads(&mut commands, stream);
            commands.remove_resource::<ConnectionReceiver>();
            next_state.set(AppState::WaitingForMatch);
        }
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d, MainCamera));
}

fn japanese_font(asset_server: &AssetServer) -> FontSource {
    FontSource::Handle(asset_server.load("fonts/NotoSansJP-Variable.ttf"))
}

fn enter_title(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Sprite::from_color(Color::srgb(0.12, 0.12, 0.2), Vec2::new(5000.0, 5000.0)),
        Transform::from_xyz(0.0, 0.0, -50.0),
        TitleEntity,
    ));

    commands.spawn((
        Text2d::new("ロバーゲーム"),
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
        Text2d::new("1/2/3/4 または ↑↓ で選択し、Enter で決定"),
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
            Transform::from_xyz(0.0, 40.0 - index as f32 * 45.0, 10.0),
            TitleMenuOption { index },
            TitleEntity,
        ));
    }

    commands.spawn((
        Text2d::new("CPU戦は先手から開始 / 対人戦はサーバーに接続"),
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

fn cleanup_title(mut commands: Commands, query: Query<Entity, With<TitleEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn title_input_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<TitleMenuSelection>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW) {
        selection.0 = (selection.0 + title_menu_labels().len() - 1) % title_menu_labels().len();
    }
    if keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS) {
        selection.0 = (selection.0 + 1) % title_menu_labels().len();
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

    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Space) {
        match selection.0 {
            0 => {
                commands.insert_resource(SelectedGameMode(GameMode::Cpu(CpuDifficulty::Weak)));
                commands.insert_resource(LocalPlayer {
                    side: Some(PlayerTurn::First),
                });
                next_state.set(AppState::Playing);
            }
            1 => {
                commands.insert_resource(SelectedGameMode(GameMode::Cpu(CpuDifficulty::Medium)));
                commands.insert_resource(LocalPlayer {
                    side: Some(PlayerTurn::First),
                });
                next_state.set(AppState::Playing);
            }
            2 => {
                commands.insert_resource(SelectedGameMode(GameMode::Cpu(CpuDifficulty::Strong)));
                commands.insert_resource(LocalPlayer {
                    side: Some(PlayerTurn::First),
                });
                next_state.set(AppState::Playing);
            }
            3 => {
                commands.insert_resource(SelectedGameMode(GameMode::OnlinePlayer));
                commands.insert_resource(LocalPlayer { side: None });
                next_state.set(AppState::Connecting);
            }
            _ => {}
        }
    }
}

fn update_title_window(mut window: Single<&mut Window, With<PrimaryWindow>>) {
    window.title = "Stomper (1/2/3:CPU 4:Player vs Player)".to_string();
}

fn setup_tcp_threads(commands: &mut Commands, stream: TcpStream) {
    let reader_stream = stream.try_clone().unwrap();
    let writer_stream = stream.try_clone().unwrap();

    let (bevy_tx, net_rx) = mpsc::channel::<GameMessage>();
    let (net_tx, bevy_rx) = mpsc::channel::<GameMessage>();

    thread::spawn(move || {
        let mut stream_clone = reader_stream;
        loop {
            let mut len_buf = [0u8; 4];
            if stream_clone.read_exact(&mut len_buf).is_err() {
                break;
            }
            let msg_len = u32::from_le_bytes(len_buf) as usize;

            let mut msg_buf = vec![0; msg_len];
            if stream_clone.read_exact(&mut msg_buf).is_err() {
                break;
            }

            if let Ok(msg) = serde_json::from_slice::<GameMessage>(&msg_buf) {
                let _ = net_tx.send(msg);
            } else {
                println!("JSONデコードエラー: 受信したデータを解析できませんでした");
            }
        }
        println!("通信が切断されました。");
    });

    thread::spawn(move || {
        let mut stream = writer_stream;
        while let Ok(msg) = net_rx.recv() {
            if let Ok(data) = serde_json::to_vec(&msg) {
                let len_bytes = (data.len() as u32).to_le_bytes();

                if stream.write_all(&len_bytes).is_err() { break; }
                if stream.write_all(&data).is_err() { break; }
                let _ = stream.flush();
            }
        }
    });

    commands.insert_resource(NetworkChannels {
        tx: Mutex::new(bevy_tx),
        rx: Mutex::new(bevy_rx),
        stream: Mutex::new(stream),
    });
}

#[derive(Component)]
struct TitleMenuOption {
    index: usize,
}

#[derive(Component)]
struct RematchPrompt;

fn title_menu_labels() -> [&'static str; 4] {
    [
        "弱いCPU",
        "中程度のCPU",
        "強いCPU",
        "プレイヤーと対戦",
    ]
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

fn setup_game(
    mut commands: Commands,
    settings: Res<TitleSettings>,
    local_player: Res<LocalPlayer>,
    mode: Res<SelectedGameMode>,
    asset_server: Res<AssetServer>,
) {
    let board = GameBoard::new_filled(settings.board_width, settings.board_height);
    let board_width = board.width();
    let board_height = board.height();
    let board_pixel_width = board_extent(board_width);
    let board_pixel_height = board_extent(board_height);

    commands.insert_resource(Gameplay {
        board,
        turn: PlayerTurn::First,
        selected: None,
        winner: None,
    });
    commands.insert_resource(TurnTimer {
        remaining_time: 30.0,
    });

    commands.spawn((
        Sprite::from_color(Color::srgba(0.2, 0.5, 1.0, 0.14), Vec2::new(5000.0, 5000.0)),
        Transform::from_xyz(0.0, 0.0, -40.0),
        TurnHighlight,
        GameEntity,
    ));

    for y in 0..board_height {
        for x in 0..board_width {
            let p = Position { x, y };
            commands.spawn((
                Sprite::from_color(Color::srgb(0.23, 0.23, 0.23), Vec2::splat(CELL_SIZE - 3.0)),
                Transform::from_translation(board_to_world(p, board_width, board_height, -10.0)),
                Cell { position: p },
                GameEntity,
            ));

            commands.spawn((
                Sprite::from_color(Color::srgb(0.08, 0.08, 0.08), Vec2::splat(CELL_SIZE * 0.56)),
                Transform::from_translation(board_to_world(p, board_width, board_height, 1.0)),
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
                Transform::from_translation(board_to_world(p, board_width, board_height, 3.0)),
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
        Transform::from_xyz(0.0, board_pixel_height / 2.0 + 80.0, 10.0),
        TimerText,
        GameEntity,
    ));

    let my_color_text = match (mode.0, local_player.side) {
        (GameMode::Cpu(difficulty), Some(PlayerTurn::First)) => {
            format!("あなた: 先手 / CPU: {}", difficulty.label())
        }
        (GameMode::Cpu(difficulty), Some(PlayerTurn::Second)) => {
            format!("あなた: 後手 / CPU: {}", difficulty.label())
        }
        (GameMode::OnlinePlayer, Some(PlayerTurn::First)) => "あなた: 先手 / 対人戦".to_string(),
        (GameMode::OnlinePlayer, Some(PlayerTurn::Second)) => "あなた: 後手 / 対人戦".to_string(),
        _ => "対戦中".to_string(),
    };

    commands.spawn((
        Text2d::new(my_color_text),
        TextFont {
            font: japanese_font(&asset_server),
            font_size: FontSize::Px(30.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(0.0, -board_pixel_height / 2.0 - 80.0, 10.0),
        PlayerInfoText,
        GameEntity,
    ));

    commands.spawn((
        Sprite::from_color(
            Color::srgba(1.0, 1.0, 1.0, 0.06),
            Vec2::new(board_pixel_width + CELL_SIZE, board_pixel_height + CELL_SIZE),
        ),
        Transform::from_xyz(0.0, 0.0, -20.0),
        GameEntity,
    ));
}

fn cleanup_game(
    mut commands: Commands,
    entities: Query<Entity, With<GameEntity>>,
    gameplay: Option<Res<Gameplay>>,
    network_opt: Option<Res<NetworkChannels>>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
    if gameplay.is_some() {
        commands.remove_resource::<Gameplay>();
    }
    commands.remove_resource::<TurnTimer>();
    if let Some(network) = network_opt {
        let _ = network.stream.lock().unwrap().shutdown(Shutdown::Both);
    }
    commands.remove_resource::<NetworkChannels>();
    commands.remove_resource::<LocalPlayer>();
    commands.remove_resource::<SelectedGameMode>();
}

fn handle_play_clicks(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera_q: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut gameplay: ResMut<Gameplay>,
    local_player: Res<LocalPlayer>,
    network_opt: Option<Res<NetworkChannels>>,
    mut turn_timer: ResMut<TurnTimer>,
) {
    if !mouse.just_pressed(MouseButton::Left) || gameplay.winner.is_some() {
        return;
    }

    if let Some(my_side) = local_player.side {
        if gameplay.turn != my_side {
            return;
        }
    }

    let cursor = match window.cursor_position() {
        Some(cursor) => cursor,
        None => return,
    };
    let (camera, camera_transform) = *camera_q;
    let Some(world_position) = camera.viewport_to_world_2d(camera_transform, cursor).ok() else {
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
            let current = gameplay.turn;
            gameplay.turn = gameplay.turn.next();
            turn_timer.remaining_time = 30.0;
            gameplay.selected = None;
            if !gameplay.board.has_legal_move() {
                gameplay.winner = Some(current);
            }

            if let Some(network) = network_opt {
                if let Ok(tx) = network.tx.lock() {
                    let _ = tx.send(GameMessage::Move {
                        from_x: selected.x,
                        from_y: selected.y,
                        to_x: clicked_cell.x,
                        to_y: clicked_cell.y,
                    });
                }
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

fn handle_network_messages(
    mut commands: Commands,
    mut gameplay: ResMut<Gameplay>,
    network_opt: Option<Res<NetworkChannels>>,
    mut turn_timer: ResMut<TurnTimer>,
    mut settings: ResMut<TitleSettings>,
    mut local_player: ResMut<LocalPlayer>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Some(network) = network_opt else { return; };
    let Ok(rx) = network.rx.lock() else { return; };

    while let Ok(msg) = rx.try_recv() {
        match msg {
            GameMessage::Move {
                from_x,
                from_y,
                to_x,
                to_y,
            } => {
                let from = Position {
                    x: from_x,
                    y: from_y,
                };
                let to = Position { x: to_x, y: to_y };

                if gameplay.board.apply_move(from, to) {
                    let current = gameplay.turn;
                    gameplay.turn = gameplay.turn.next();
                    turn_timer.remaining_time = 30.0;
                    gameplay.selected = None;
                    if !gameplay.board.has_legal_move() {
                        gameplay.winner = Some(current);
                    }
                }
            }
            GameMessage::MatchStarted {
                side,
                board_width,
                board_height,
            } => {
                local_player.side = Some(side);
                settings.board_width = board_width;
                settings.board_height = board_height;
                reset_match(&mut gameplay, &mut turn_timer);
            }
            GameMessage::RematchDecision { continue_playing } => {
                if continue_playing {
                    reset_match(&mut gameplay, &mut turn_timer);
                } else {
                    let _ = network.stream.lock().unwrap().shutdown(Shutdown::Both);
                    commands.remove_resource::<NetworkChannels>();
                    commands.remove_resource::<LocalPlayer>();
                    commands.remove_resource::<SelectedGameMode>();
                    next_state.set(AppState::Title);
                }
            }
            GameMessage::OpponentDisconnected => {
                let _ = network.stream.lock().unwrap().shutdown(Shutdown::Both);
                commands.remove_resource::<NetworkChannels>();
                commands.remove_resource::<LocalPlayer>();
                commands.remove_resource::<SelectedGameMode>();
                next_state.set(AppState::Title);
            }
            GameMessage::WaitingForOpponent => {}
        }
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
    if gameplay.winner.is_some() || gameplay.turn != cpu_side {
        return;
    }

    let Some((from, to)) = choose_cpu_move_via_agent(&gameplay.board, difficulty) else {
        gameplay.winner = Some(human_side);
        return;
    };

    if gameplay.board.apply_move(from, to) {
        let current = gameplay.turn;
        gameplay.turn = gameplay.turn.next();
        turn_timer.remaining_time = 30.0;
        gameplay.selected = None;
        if !gameplay.board.has_legal_move() {
            gameplay.winner = Some(current);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn choose_cpu_move_via_agent(
    board: &GameBoard,
    difficulty: CpuDifficulty,
) -> Option<(Position, Position)> {
    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/rl_agent/cpu_move.py");
    let script_dir = script_path.parent().unwrap().to_path_buf();
    let board_state = board_state_json(board);
    let difficulty_arg = match difficulty {
        CpuDifficulty::Weak => "weak",
        CpuDifficulty::Medium => "medium",
        CpuDifficulty::Strong => "strong",
    };

    let output = match Command::new("uv")
        .current_dir(&script_dir)
        .args([
            "run",
            "python",
            "cpu_move.py",
            "--agent_name",
            "AlphaStomper",
            "--width",
        ])
        .arg(board.width().to_string())
        .args(["--height"])
        .arg(board.height().to_string())
        .args(["--difficulty"])
        .arg(difficulty_arg)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()
        .and_then(|mut child| {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(board_state.as_bytes());
            }
            child.wait_with_output().ok()
        }) {
        Some(output) => output,
        None => return choose_cpu_move_fallback(board),
    };

    if !output.status.success() {
        eprintln!(
            "CPU agent failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return choose_cpu_move_fallback(board);
    }

    let response = String::from_utf8(output.stdout).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&response).ok()?;
    let from = Position {
        x: parsed.get("from_x")?.as_u64()? as usize,
        y: parsed.get("from_y")?.as_u64()? as usize,
    };
    let to = Position {
        x: parsed.get("to_x")?.as_u64()? as usize,
        y: parsed.get("to_y")?.as_u64()? as usize,
    };
    Some((from, to))
}

#[cfg(target_arch = "wasm32")]
fn choose_cpu_move_via_agent(
    board: &GameBoard,
    difficulty: CpuDifficulty,
) -> Option<(Position, Position)> {
    choose_cpu_move(board, difficulty)
}

fn choose_cpu_move_fallback(board: &GameBoard) -> Option<(Position, Position)> {
    choose_cpu_move(board, CpuDifficulty::Weak)
}

fn board_state_json(board: &GameBoard) -> String {
    let mut values = Vec::with_capacity(board.width() * board.height());
    for y in 0..board.height() {
        for x in 0..board.width() {
            values.push(board.stack_at(Position { x, y }) as u8);
        }
    }
    serde_json::json!({
        "width": board.width(),
        "height": board.height(),
        "board_state": values,
    })
    .to_string()
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
        CpuDifficulty::Weak => moves.choose(&mut rng).copied(),
        CpuDifficulty::Medium => {
            let scored = moves
                .into_iter()
                .map(|(from, to)| ((from, to), score_move(board, from, to)))
                .collect::<Vec<_>>();
            let min_opponent = scored.iter().map(|(_, score)| score.opponent_moves).min()?;
            let mut best = scored
                .into_iter()
                .filter(|(_, score)| score.opponent_moves == min_opponent)
                .collect::<Vec<_>>();
            best.shuffle(&mut rng);
            best.first().map(|(mv, _)| *mv)
        }
        CpuDifficulty::Strong => moves
            .into_iter()
            .max_by_key(|(from, to)| {
                let score = score_move(board, *from, *to);
                (
                    Reverse(score.opponent_moves),
                    score.resulting_stack,
                    score.moving_stack,
                )
            }),
    }
}

#[derive(Clone, Copy)]
struct MoveScore {
    opponent_moves: usize,
    resulting_stack: u32,
    moving_stack: u32,
}

fn score_move(board: &GameBoard, from: Position, to: Position) -> MoveScore {
    let moving_stack = board.stack_at(from);
    let resulting_stack = moving_stack + board.stack_at(to);
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
    mode: Option<Res<SelectedGameMode>>,
    mut gameplay: ResMut<Gameplay>,
    local_player: Option<Res<LocalPlayer>>,
    network_opt: Option<Res<NetworkChannels>>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut turn_timer: ResMut<TurnTimer>,
) {
    let Some(mode) = mode else { return; };
    if !matches!(mode.0, GameMode::OnlinePlayer) {
        return;
    }

    let Some(local_player) = local_player else { return; };
    let Some(winner) = gameplay.winner else { return; };
    if local_player.side == Some(winner) {
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyC) || keyboard.just_pressed(KeyCode::Enter) {
        if let Some(network) = network_opt {
            if let Ok(tx) = network.tx.lock() {
                let _ = tx.send(GameMessage::RematchDecision {
                    continue_playing: true,
                });
            }
        }
        reset_match(&mut gameplay, &mut turn_timer);
    } else if keyboard.just_pressed(KeyCode::KeyQ) || keyboard.just_pressed(KeyCode::Escape) {
        if let Some(network) = network_opt {
            if let Ok(tx) = network.tx.lock() {
                let _ = tx.send(GameMessage::RematchDecision {
                    continue_playing: false,
                });
            }
            let _ = network.stream.lock().unwrap().shutdown(Shutdown::Both);
        }
        commands.remove_resource::<NetworkChannels>();
        commands.remove_resource::<LocalPlayer>();
        commands.remove_resource::<SelectedGameMode>();
        next_state.set(AppState::Title);
    }
}

fn sync_rematch_prompt(
    mut commands: Commands,
    gameplay: Option<Res<Gameplay>>,
    local_player: Option<Res<LocalPlayer>>,
    mode: Option<Res<SelectedGameMode>>,
    prompt_entities: Query<Entity, With<RematchPrompt>>,
    asset_server: Res<AssetServer>,
) {
    let should_show = matches!(mode.as_deref(), Some(mode) if matches!(mode.0, GameMode::OnlinePlayer))
        && matches!(gameplay.as_deref(), Some(gameplay) if gameplay.winner.is_some())
        && local_player.is_some();

    if should_show {
        if prompt_entities.is_empty() {
            let gameplay = gameplay.as_ref().unwrap();
            let local_player = local_player.as_ref().unwrap();
            let winner = gameplay.winner.unwrap();
            let board_pixel_height = board_extent(gameplay.board.height());
            let status = if local_player.side == Some(winner) {
                "相手の選択を待っています"
            } else {
                "C: 続ける / Q: やめる"
            };

            commands.spawn((
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.55), Vec2::new(700.0, 170.0)),
                Transform::from_xyz(0.0, board_pixel_height / 2.0 + 20.0, 20.0),
                RematchPrompt,
                GameEntity,
            ));
            commands.spawn((
                Text2d::new("対局終了"),
                TextFont {
                    font: japanese_font(&asset_server),
                    font_size: FontSize::Px(34.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_xyz(0.0, board_pixel_height / 2.0 + 70.0, 25.0),
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
                Transform::from_xyz(0.0, board_pixel_height / 2.0 + 20.0, 25.0),
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
    mode: Option<Res<SelectedGameMode>>,
    gameplay: Res<Gameplay>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let Some(mode) = mode {
        if matches!(mode.0, GameMode::Cpu(_))
            && gameplay.winner.is_some()
            && (keyboard.just_pressed(KeyCode::KeyR) || keyboard.just_pressed(KeyCode::Escape))
        {
            next_state.set(AppState::Title);
        }
    }

    if gameplay.winner.is_none()
        && (keyboard.just_pressed(KeyCode::KeyR) || keyboard.just_pressed(KeyCode::Escape))
    {
        next_state.set(AppState::Title);
    }
}

fn update_board_visuals(
    gameplay: Res<Gameplay>,
    mut cells: Query<(&Cell, &mut Sprite), Without<StoneVisual>>,
    mut stones: Query<(&StoneVisual, &mut Sprite, &mut Transform), Without<Cell>>,
    mut stone_texts: Query<(&StoneCountText, &mut Text2d), Without<Cell>>,
) {
    for (cell, mut sprite) in &mut cells {
        let mut color = Color::srgb(0.23, 0.23, 0.23);
        if gameplay.selected == Some(cell.position) {
            color = Color::srgb(0.14, 0.6, 0.2);
        }
        sprite.color = color;
    }

    for (stone, mut sprite, mut transform) in &mut stones {
        let height = gameplay.board.stack_at(stone.position);
        if height == 0 {
            sprite.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
            transform.scale = Vec3::ONE;
            continue;
        }
        sprite.color = Color::srgb(0.04, 0.04, 0.04);
        let scale = 0.45 + (height as f32).sqrt() * 0.2;
        transform.scale = Vec3::splat(scale.min(1.7));
    }

    for (stone_text, mut text) in &mut stone_texts {
        let height = gameplay.board.stack_at(stone_text.position);
        text.0 = if height == 0 {
            String::new()
        } else {
            height.to_string()
        };
    }
}

fn update_connecting_window(mut window: Single<&mut Window, With<PrimaryWindow>>) {
    window.title = "サーバーに接続中... (Escでキャンセル)".to_string();
}

fn connecting_keyboard_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
    network_opt: Option<Res<NetworkChannels>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        commands.remove_resource::<ConnectionReceiver>();
        if let Some(network) = network_opt {
            let _ = network.stream.lock().unwrap().shutdown(Shutdown::Both);
            commands.remove_resource::<NetworkChannels>();
        }
        commands.remove_resource::<SelectedGameMode>();
        commands.remove_resource::<LocalPlayer>();
        next_state.set(AppState::Title);
    }
}

fn update_turn_highlight(
    gameplay: Res<Gameplay>,
    mut query: Single<&mut Sprite, With<TurnHighlight>>,
) {
    query.color = gameplay.turn.tint();
}

fn update_turn_timer(
    time: Res<Time>,
    mut timer_res: ResMut<TurnTimer>,
    mut timer_text_query: Query<&mut Text2d, With<TimerText>>,
    gameplay: Res<Gameplay>,
) {
    if gameplay.winner.is_some() {
        return;
    }

    timer_res.remaining_time -= time.delta_secs();
    if timer_res.remaining_time <= 0.0 {
        timer_res.remaining_time = 0.0;
    }

    for mut text in &mut timer_text_query {
        text.0 = format!("Time: {:.1}", timer_res.remaining_time);
    }
}

fn update_game_window_title(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    gameplay: Res<Gameplay>,
    local_player: Res<LocalPlayer>,
    mode: Res<SelectedGameMode>,
) {
    let mode_str = match mode.0 {
        GameMode::Cpu(difficulty) => format!("[{}]", difficulty.label()),
        GameMode::OnlinePlayer => match local_player.side {
            Some(PlayerTurn::First) => "[対人戦: あなたは先手]".to_string(),
            Some(PlayerTurn::Second) => "[対人戦: あなたは後手]".to_string(),
            None => "[対人戦]".to_string(),
        },
    };

    if let Some(winner) = gameplay.winner {
        window.title = format!(
            "Stomper Game {} | {} の勝ち",
            mode_str,
            winner.label()
        );
    } else {
        window.title = format!(
            "Stomper Game {} | {} の手番 (盤面 {}x{})",
            mode_str,
            gameplay.turn.label(),
            gameplay.board.width(),
            gameplay.board.height()
        );
    }
}

fn board_extent(size: usize) -> f32 {
    size as f32 * CELL_SIZE
}

fn board_to_world(pos: Position, width: usize, height: usize, z: f32) -> Vec3 {
    let half_width = board_extent(width) / 2.0;
    let half_height = board_extent(height) / 2.0;
    let x = (pos.x as f32 + 0.5) * CELL_SIZE - half_width;
    let y = (pos.y as f32 + 0.5) * CELL_SIZE - half_height;
    Vec3::new(x, y, z)
}

fn world_to_board(world: Vec2, width: usize, height: usize) -> Option<Position> {
    let half_width = board_extent(width) / 2.0;
    let half_height = board_extent(height) / 2.0;
    let x = ((world.x + half_width) / CELL_SIZE).floor() as isize;
    let y = ((world.y + half_height) / CELL_SIZE).floor() as isize;
    if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
        return None;
    }
    Some(Position {
        x: x as usize,
        y: y as usize,
    })
}

fn wait_for_opponent(
    network_opt: Option<Res<NetworkChannels>>,
    mut local_player: ResMut<LocalPlayer>,
    mut settings: ResMut<TitleSettings>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    let Some(network) = network_opt else { return; };
    let Ok(rx) = network.rx.lock() else { return; };

    while let Ok(msg) = rx.try_recv() {
        match msg {
            GameMessage::MatchStarted {
                side,
                board_width,
                board_height,
            } => {
                println!("🎉 マッチング成立！ あなたは {:?} です", side);
                local_player.side = Some(side);
                settings.board_width = board_width;
                settings.board_height = board_height;
                next_state.set(AppState::Playing);
            }
            GameMessage::OpponentDisconnected => {
                let _ = network.stream.lock().unwrap().shutdown(Shutdown::Both);
                commands.remove_resource::<NetworkChannels>();
                commands.remove_resource::<SelectedGameMode>();
                commands.remove_resource::<LocalPlayer>();
                next_state.set(AppState::Title);
            }
            GameMessage::WaitingForOpponent => {}
            GameMessage::Move { .. } | GameMessage::RematchDecision { .. } => {}
        }
    }
}

fn update_waiting_window(mut window: Single<&mut Window, With<PrimaryWindow>>) {
    window.title = "対戦相手を待っています... (Escでキャンセル)".to_string();
}

fn waiting_keyboard_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
    network_opt: Option<Res<NetworkChannels>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        commands.remove_resource::<ConnectionReceiver>();
        if let Some(network) = network_opt {
            let _ = network.stream.lock().unwrap().shutdown(Shutdown::Both);
            commands.remove_resource::<NetworkChannels>();
        }
        commands.remove_resource::<SelectedGameMode>();
        commands.remove_resource::<LocalPlayer>();
        next_state.set(AppState::Title);
    }
}

fn reset_match(gameplay: &mut Gameplay, turn_timer: &mut TurnTimer) {
    let width = gameplay.board.width();
    let height = gameplay.board.height();
    gameplay.board = GameBoard::new_filled(width, height);
    gameplay.turn = PlayerTurn::First;
    gameplay.selected = None;
    gameplay.winner = None;
    turn_timer.remaining_time = 30.0;
}