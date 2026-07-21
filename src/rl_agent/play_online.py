import socket
import struct
import json
import torch
import os

from rl_agent import RLAgent

# ==========================================
# TODO: Rust側で起動しているゲームとサイズを合わせる
BOARD_WIDTH = 4
BOARD_HEIGHT = 1
# ==========================================

def recvall(sock, n):
    data = bytearray()
    while len(data) < n:
        packet = sock.recv(n - len(data))
        if not packet:
            return None
        data.extend(packet)
    return data

def send_move(sock, from_x, from_y, to_x, to_y):
    msg_dict = {
        "Move": {
            "from_x": from_x,
            "from_y": from_y,
            "to_x": to_x,
            "to_y": to_y
        }
    }
    msg_bytes = json.dumps(msg_dict).encode('utf-8')
    length_bytes = struct.pack('<I', len(msg_bytes))
    sock.sendall(length_bytes + msg_bytes)

def main():
    HOST = '127.0.0.1'
    PORT = 8080

    # 指定したサイズでエージェントを初期化
    agent = RLAgent(BOARD_WIDTH, BOARD_HEIGHT)
    weight_path = f"weights_{BOARD_WIDTH}x{BOARD_HEIGHT}.pth"

    if os.path.exists(weight_path):
        agent.load_state_dict(torch.load(weight_path, weights_only=True))
        agent.eval()
        print(f"モデル '{weight_path}' をロードしました！")
    else:
        print(f"警告: モデル '{weight_path}' が見つかりません。ランダム行動します。")

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, PORT))
        print(f"Rustサーバーに接続しました (想定サイズ: {BOARD_WIDTH}x{BOARD_HEIGHT})")

        while True:
            length_bytes = recvall(s, 4)
            if not length_bytes: break

            msg_len = struct.unpack('<I', length_bytes)[0]
            msg_bytes = recvall(s, msg_len)
            if not msg_bytes: break

            try:
                msg = json.loads(msg_bytes.decode('utf-8'))
            except Exception:
                continue

            if "StateRequest" in msg:
                req = msg["StateRequest"]
                board_state = req["board_state"]

                # 安全確認：Rust側から送られてきた盤面サイズと設定が一致しているか
                if len(board_state) != BOARD_WIDTH * BOARD_HEIGHT:
                    print(f"エラー: 受信した配列サイズ({len(board_state)})が設定({BOARD_WIDTH*BOARD_HEIGHT})と違います！")
                    continue

                if req["is_done"]:
                    print(f"ゲーム終了")
                    continue

                action_info = agent.select_action(board_state, epsilon=0.0)
                if action_info:
                    send_move(s, action_info["from_x"], action_info["from_y"], action_info["to_x"], action_info["to_y"])

if __name__ == "__main__":
    main()