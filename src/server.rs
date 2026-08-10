use rand::Rng;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

mod protocol;

use protocol::{GameMessage, PlayerTurn};

fn send_message(stream: &mut TcpStream, msg: &GameMessage) -> std::io::Result<()> {
    let data = serde_json::to_vec(msg)?;
    let len_bytes = (data.len() as u32).to_le_bytes();
    stream.write_all(&len_bytes)?;
    stream.write_all(&data)?;
    stream.flush()?;
    Ok(())
}

fn relay_messages(mut read_stream: TcpStream, mut write_stream: TcpStream) {
    let _ = read_stream.set_nonblocking(false);
    let _ = write_stream.set_nonblocking(false);

    loop {
        let mut len_buf = [0u8; 4];
        if read_stream.read_exact(&mut len_buf).is_err() {
            let _ = send_message(&mut write_stream, &GameMessage::OpponentDisconnected);
            break;
        }

        let msg_len = u32::from_le_bytes(len_buf) as usize;
        let mut msg_buf = vec![0; msg_len];
        if read_stream.read_exact(&mut msg_buf).is_err() {
            let _ = send_message(&mut write_stream, &GameMessage::OpponentDisconnected);
            break;
        }

        if let Ok(msg) = serde_json::from_slice::<GameMessage>(&msg_buf) {
            if send_message(&mut write_stream, &msg).is_err() {
                break;
            }
        }
    }
}

fn parse_board_size() -> (usize, usize) {
    let mut args = std::env::args().skip(1);
    let width = args.next().and_then(|v| v.parse().ok()).unwrap_or(4);
    let height = args.next().and_then(|v| v.parse().ok()).unwrap_or(4);
    (width.max(1), height.max(1))
}

fn main() -> std::io::Result<()> {
    let (board_width, board_height) = parse_board_size();
    let listener = TcpListener::bind("172.192.161.164:8080")?;
    listener.set_nonblocking(true)?;

    println!("========================================");
    println!(
        "TCPサーバー起動 (port: 8080, board: {}x{})",
        board_width, board_height
    );
    println!("========================================");

    let mut waiting_player: Option<TcpStream> = None;
    let mut rng = rand::thread_rng();

    loop {
        match listener.accept() {
            Ok((mut stream, addr)) => {
                println!("クライアント接続: {}", addr);

                if let Some(mut first_player) = waiting_player.take() {
                    let first_is_player1 = rng.gen_bool(0.5);
                    let (first_side, second_side) = if first_is_player1 {
                        (PlayerTurn::First, PlayerTurn::Second)
                    } else {
                        (PlayerTurn::Second, PlayerTurn::First)
                    };

                    if send_message(
                        &mut first_player,
                        &GameMessage::MatchStarted {
                            side: first_side,
                            board_width,
                            board_height,
                        },
                    )
                    .is_err()
                    {
                        waiting_player = Some(stream);
                        continue;
                    }

                    if send_message(
                        &mut stream,
                        &GameMessage::MatchStarted {
                            side: second_side,
                            board_width,
                            board_height,
                        },
                    )
                    .is_err()
                    {
                        waiting_player = Some(first_player);
                        continue;
                    }

                    println!(
                        "マッチ成立: 先の接続は {:?}, 後の接続 {} は {:?}",
                        first_side, addr, second_side
                    );

                    let p1_read = first_player.try_clone()?;
                    let p2_write = stream.try_clone()?;
                    thread::spawn(move || relay_messages(p1_read, p2_write));

                    let p2_read = stream.try_clone()?;
                    let p1_write = first_player.try_clone()?;
                    thread::spawn(move || relay_messages(p2_read, p1_write));
                } else {
                    println!("待機中のプレイヤーを登録");
                    let _ = send_message(&mut stream, &GameMessage::WaitingForOpponent);
                    waiting_player = Some(stream);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("accept error: {e}"),
        }

        thread::sleep(Duration::from_millis(10));
    }
}
