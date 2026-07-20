use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, mpsc::{self, Receiver, Sender}};
use std::thread;

mod game_logic;
use game_logic::{GameBoard, Position};

const CELL_SIZE: f32 = 72.0;
const TITLE_MIN_SIZE: usize = 2;
const TITLE_MAX_SIZE: usize = 12;

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
enum PlayerTurn {
    First,
    Second,
}

impl PlayerTurn {
    fn next(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::First => "先手",
            Self::Second => "後手",
        }
    }

    fn tint(self) -> Color {
        match self {
            Self::First => Color::srgba(0.2, 0.5, 1.0, 0.14),
            Self::Second => Color::srgba(1.0, 0.45, 0.2, 0.14),
        }
    }
}

// ----------------------------------------------------------------
// ネットワーク用のデータ構造
// ----------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
enum GameMessage {
    // game_logic::Position側にSerializeが実装されていないことを想定し，
    // usizeで座標データを送受信
    Move {
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NetworkMode {
    Offline,
    Host,
    Join,
}

#[derive(Resource)]
struct SelectedNetworkMode(NetworkMode);

#[derive(Resource)]
struct LocalPlayer {
    side: Option<PlayerTurn>, // Noneの場合はオフライン(両方の手を操作可能)
}

#[derive(Resource)]
struct NetworkChannels {
    tx: Mutex<Sender<GameMessage>>,
    rx: Mutex<Receiver<GameMessage>>,
}

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
            board_width: 5,
            board_height: 6,
        })
        .add_plugins(DefaultPlugins)
        .init_state::<AppState>()
        .add_systems(Startup, setup_camera)
        .add_systems(
            Update,
            (title_input_system, update_title_window).run_if(in_state(AppState::Title)),
        )
        .add_systems(OnEnter(AppState::Title), enter_title)
        .add_systems(OnExit(AppState::Title), cleanup_title)
        .add_systems(OnEnter(AppState::Playing), (setup_network, setup_game).chain())
        .add_systems(
            Update,
            (
                handle_play_clicks,
                handle_network_messages, // ネットワーク受信処理を追加
                playing_keyboard_shortcuts,
                update_board_visuals,
                update_turn_highlight,
                update_game_window_title,
            )
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(OnExit(AppState::Playing), cleanup_game)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d, MainCamera));
}

fn enter_title(mut commands: Commands) {
    commands.spawn((
        Sprite::from_color(Color::srgb(0.12, 0.12, 0.2), Vec2::new(5000.0, 5000.0)),
        Transform::from_xyz(0.0, 0.0, -50.0),
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
    mut next_state: ResMut<NextState<AppState>>,
    mut settings: ResMut<TitleSettings>,
) {
    if keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::Equal) {
        settings.board_height = (settings.board_height + 1).min(TITLE_MAX_SIZE);
    }
    if keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::Minus) {
        settings.board_height = settings
            .board_height
            .saturating_sub(1)
            .max(TITLE_MIN_SIZE);
    }
    if keyboard.just_pressed(KeyCode::ArrowRight) {
        settings.board_width = (settings.board_width + 1).min(TITLE_MAX_SIZE);
    }
    if keyboard.just_pressed(KeyCode::ArrowLeft) {
        settings.board_width = settings
            .board_width
            .saturating_sub(1)
            .max(TITLE_MIN_SIZE);
    }

    // キー入力でモードを決定してゲーム開始
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Space) {
        commands.insert_resource(SelectedNetworkMode(NetworkMode::Offline));
        next_state.set(AppState::Playing);
    } else if keyboard.just_pressed(KeyCode::KeyH) {
        commands.insert_resource(SelectedNetworkMode(NetworkMode::Host));
        next_state.set(AppState::Playing);
    } else if keyboard.just_pressed(KeyCode::KeyJ) {
        commands.insert_resource(SelectedNetworkMode(NetworkMode::Join));
        next_state.set(AppState::Playing);
    }
}

fn update_title_window(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    settings: Res<TitleSettings>,
) {
    window.title = format!(
        "Robber Game | サイズ {}x{} (矢印:変更 / Enter:オフライン / H:ホスト作成 / J:参加)",
        settings.board_width,
        settings.board_height
    );
}

// ----------------------------------------------------------------
// ネットワーク接続のセットアップ
// ----------------------------------------------------------------
fn setup_network(mut commands: Commands, mode_res: Res<SelectedNetworkMode>) {
    match mode_res.0 {
        NetworkMode::Offline => {
            commands.insert_resource(LocalPlayer { side: None });
        }
        NetworkMode::Host => {
            println!("ポート8080でクライアントの接続を待機中...");
            let listener = TcpListener::bind("0.0.0.0:8080").expect("ポートのバインドに失敗しました");
            let (stream, addr) = listener.accept().expect("接続の受け入れに失敗しました"); // ← mut を削除
            println!("クライアントが接続しました: {}", addr);

            setup_tcp_threads(&mut commands, stream);
            commands.insert_resource(LocalPlayer { side: Some(PlayerTurn::First) }); // ホストが先手
        }
        NetworkMode::Join => {
            println!("サーバー(127.0.0.1:8080)に接続中...");
            // Todo: 試しに実装したものであるため，接続に失敗するとパニックする．UIでエラーを出す処理が必要
            let stream = TcpStream::connect("127.0.0.1:8080").expect("サーバーへの接続に失敗しました");
            println!("サーバーに接続しました！");

            setup_tcp_threads(&mut commands, stream);
            commands.insert_resource(LocalPlayer { side: Some(PlayerTurn::Second) }); // 参加者が後手
        }
    }
}

// TCPの送受信スレッドを立ち上げるヘルパー関数
fn setup_tcp_threads(commands: &mut Commands, mut stream: TcpStream) {
    let mut stream_clone = stream.try_clone().unwrap();

    let (bevy_tx, net_rx) = mpsc::channel::<GameMessage>();
    let (net_tx, bevy_rx) = mpsc::channel::<GameMessage>();

    // 受信スレッド
    thread::spawn(move || {
        let mut buffer = [0; 512];
        loop {
            if let Ok(size) = stream_clone.read(&mut buffer) {
                if size == 0 { break; } // 切断検知
                if let Ok(msg) = bincode::deserialize::<GameMessage>(&buffer[..size]) {
                    let _ = net_tx.send(msg);
                }
            } else {
                break;
            }
        }
        println!("通信が切断されました．");
    });

    // 送信スレッド
    thread::spawn(move || {
        while let Ok(msg) = net_rx.recv() {
            if let Ok(data) = bincode::serialize(&msg) {
                let _ = stream.write_all(&data);
            }
        }
    });

    commands.insert_resource(NetworkChannels {
        tx: Mutex::new(bevy_tx),
        rx: Mutex::new(bevy_rx),
    });
}
// ----------------------------------------------------------------


fn setup_game(mut commands: Commands, settings: Res<TitleSettings>) {
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
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
    if gameplay.is_some() {
        commands.remove_resource::<Gameplay>();
    }
    // ネットワークリソースのお掃除
    commands.remove_resource::<NetworkChannels>();
    commands.remove_resource::<LocalPlayer>();
    commands.remove_resource::<SelectedNetworkMode>();
}

fn handle_play_clicks(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera_q: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut gameplay: ResMut<Gameplay>,
    local_player: Res<LocalPlayer>,
    network_opt: Option<Res<NetworkChannels>>, // オフライン時は存在しない
) {
    if !mouse.just_pressed(MouseButton::Left) || gameplay.winner.is_some() {
        return;
    }

    // オンライン対戦中かつ，自分のターンではない場合は入力を弾く
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

        // ローカルで手を適用
        if gameplay.board.apply_move(selected, clicked_cell) {
            let current = gameplay.turn;
            gameplay.turn = gameplay.turn.next();
            gameplay.selected = None;
            if !gameplay.board.has_legal_move() {
                gameplay.winner = Some(current);
            }

            // オンラインであれば，適用した手を相手に送信する
            if let Some(network) = network_opt {
                let msg = GameMessage::Move {
                    from_x: selected.x,
                    from_y: selected.y,
                    to_x: clicked_cell.x,
                    to_y: clicked_cell.y,
                };
                // Mutexをロックしてから送信する
                if let Ok(tx) = network.tx.lock() {
                    let _ = tx.send(msg);
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

// ----------------------------------------------------------------
// ネットワークからの受信処理
// ----------------------------------------------------------------
fn handle_network_messages(
    mut gameplay: ResMut<Gameplay>,
    network_opt: Option<Res<NetworkChannels>>,
) {
    let Some(network) = network_opt else { return; };

    // Mutexをロックして Receiver にアクセス
    let Ok(rx) = network.rx.lock() else { return; };

    // キューに溜まっているすべてのメッセージを処理する
    while let Ok(msg) = rx.try_recv() {
        match msg {
            GameMessage::Move { from_x, from_y, to_x, to_y } => {
                let from = Position { x: from_x, y: from_y };
                let to = Position { x: to_x, y: to_y };

                // 受信した手で盤面を更新
                if gameplay.board.apply_move(from, to) {
                    let current = gameplay.turn;
                    gameplay.turn = gameplay.turn.next();
                    gameplay.selected = None;
                    if !gameplay.board.has_legal_move() {
                        gameplay.winner = Some(current);
                    }
                }
            }
        }
    }
}

fn playing_keyboard_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) || keyboard.just_pressed(KeyCode::Escape) {
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

fn update_turn_highlight(
    gameplay: Res<Gameplay>,
    mut query: Single<&mut Sprite, With<TurnHighlight>>,
) {
    query.color = gameplay.turn.tint();
}

fn update_game_window_title(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    gameplay: Res<Gameplay>,
    local_player: Res<LocalPlayer>,
) {
    let mode_str = match local_player.side {
        Some(PlayerTurn::First) => "[ホスト: あなたは先手]",
        Some(PlayerTurn::Second) => "[参加者: あなたは後手]",
        None => "[オフライン]",
    };

    if let Some(winner) = gameplay.winner {
        window.title = format!(
            "Robber Game {} | {} の勝ち (Rでタイトルへ戻る)",
            mode_str,
            winner.label()
        );
    } else {
        window.title = format!(
            "Robber Game {} | {} の手番をハイライト中 (盤面 {}x{})",
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