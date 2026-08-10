import socket
import struct
import json
import torch
import os
import sys
import argparse  # ★追加: コマンドライン引数を処理するモジュール
from pathlib import Path

from rl_agent import RLAgent

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

def has_legal_move(board_state, width, height):
    """盤面に合法手（動かせる石）が残っているか判定する"""
    for from_y in range(height):
        for from_x in range(width):
            from_idx = from_y * width + from_x
            from_stack = board_state[from_idx]

            if from_stack == 0:
                continue

            for dx, dy in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                to_x = from_x + dx
                to_y = from_y + dy

                if 0 <= to_x < width and 0 <= to_y < height:
                    to_idx = to_y * width + to_x
                    to_stack = board_state[to_idx]

                    if to_stack > 0 and from_stack >= to_stack:
                        return True
    return False

def main():
    # ==========================================
    # ★ コマンドライン引数の設定
    # ==========================================
    parser = argparse.ArgumentParser(description="オンライン対戦用 AIクライアント")
    parser.add_argument("--agent_name", type=str, default="AlphaStomper", help="エージェント名")
    parser.add_argument("--width", type=int, default=4, help="盤面の幅")
    parser.add_argument("--height", type=int, default=4, help="盤面の高さ")
    parser.add_argument("--episodes", type=int, default=100, help="学習回数（読み込むモデルの指定用）")

    args = parser.parse_args()

    AGENT_NAME = args.agent_name
    BOARD_WIDTH = args.width
    BOARD_HEIGHT = args.height
    TRAIN_EPISODES = args.episodes

    HOST = '127.0.0.1'
    PORT = 8080

    agent = RLAgent(BOARD_WIDTH, BOARD_HEIGHT)

    # 命名規則: 「エージェント名_幅x高さ_学習回数.pkl」
    weight_path = Path(__file__).resolve().parent / f"{AGENT_NAME}_{BOARD_WIDTH}x{BOARD_HEIGHT}_{TRAIN_EPISODES}.pkl"

    if weight_path.exists():
        checkpoint = torch.load(weight_path, weights_only=False)

        if isinstance(checkpoint, dict) and "model_state_dict" in checkpoint:
            agent.load_state_dict(checkpoint["model_state_dict"])
            if checkpoint.get("is_perfect_agent", False):
                print(f"✨【完全解析モデル】'{weight_path.name}' をロードしました！")
                print("🤖 このエージェントはどんな時でも最善手を打ちます。")
            else:
                print(f"✅ モデル '{weight_path.name}' をロードしました。（学習中モデル）")
        else:
            agent.load_state_dict(checkpoint)
            print(f"✅ 旧形式のモデル '{weight_path.name}' をロードしました。")

        agent.eval()
    else:
        print(f"⚠️ 警告: モデル '{weight_path.name}' が見つかりません。ランダム行動します。")

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        try:
            s.connect((HOST, PORT))
            print(f"Rustサーバーに接続しました (想定サイズ: {BOARD_WIDTH}x{BOARD_HEIGHT})")
        except ConnectionRefusedError:
            print("エラー: サーバーに接続できません。Rust側のゲームが起動しているか確認してください。")
            sys.exit(1)

        while True:
            length_bytes = recvall(s, 4)
            if not length_bytes:
                print("サーバーとの接続が切断されました。プログラムを終了します。")
                break

            msg_len = struct.unpack('<I', length_bytes)[0]
            msg_bytes = recvall(s, msg_len)
            if not msg_bytes:
                print("サーバーとの接続が切断されました。プログラムを終了します。")
                break

            try:
                msg = json.loads(msg_bytes.decode('utf-8'))
            except Exception:
                print("受信したメッセージのJSONデコードに失敗しました。")
                sys.exit(1)

            if "StateRequest" in msg:
                req = msg["StateRequest"]
                board_state = req["board_state"]

                if len(board_state) != BOARD_WIDTH * BOARD_HEIGHT:
                    print(f"エラー: 受信した配列サイズ({len(board_state)})が設定({BOARD_WIDTH*BOARD_HEIGHT})と違います！")
                    sys.exit(1)

                if req.get("is_done"):
                    winner = req.get("winner")
                    if winner == "First":
                        print("ゲーム終了: 先手の勝利")
                    elif winner == "Second":
                        print("ゲーム終了: 後手の勝利")
                    else:
                        print("ゲーム終了")
                    sys.exit(0)

                action_info = agent.select_action(board_state, epsilon=0.0)
                if action_info:
                    send_move(s, action_info["from_x"], action_info["from_y"], action_info["to_x"], action_info["to_y"])

                    from_idx = action_info["from_y"] * BOARD_WIDTH + action_info["from_x"]
                    to_idx = action_info["to_y"] * BOARD_WIDTH + action_info["to_x"]
                    board_state[to_idx] += board_state[from_idx]
                    board_state[from_idx] = 0

                    if not has_legal_move(board_state, BOARD_WIDTH, BOARD_HEIGHT):
                        print("ゲーム終了: 後手の勝利")
                        sys.exit(0)

if __name__ == "__main__":
    main()