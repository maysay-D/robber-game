import argparse
import json
import re
import sys
from pathlib import Path

import torch

from rl_agent import RLAgent


MODEL_RE = re.compile(r"^(?P<name>.+)_(?P<width>\d+)x(?P<height>\d+)_(?P<episodes>\d+)\.pkl$")


def resolve_model_path(base_dir: Path, agent_name: str, width: int, height: int, difficulty: str) -> Path | None:
    models = []
    for path in base_dir.glob(f"{agent_name}_{width}x{height}_*.pkl"):
        match = MODEL_RE.match(path.name)
        if not match:
            continue
        models.append((int(match.group("episodes")), path))

    models.sort(key=lambda item: item[0])
    if not models:
        return None

    if difficulty == "weak":
        return models[0][1]
    if difficulty == "strong":
        return models[-1][1]
    return models[len(models) // 2][1]


def load_agent(model_path: Path, width: int, height: int) -> RLAgent:
    agent = RLAgent(width, height)
    checkpoint = torch.load(model_path, map_location="cpu", weights_only=False)
    if isinstance(checkpoint, dict) and "model_state_dict" in checkpoint:
        agent.load_state_dict(checkpoint["model_state_dict"])
    else:
        agent.load_state_dict(checkpoint)
    agent.eval()
    return agent


def choose_random_move(board_state, width, height):
    moves = []
    for from_y in range(height):
        for from_x in range(width):
            from_idx = from_y * width + from_x
            from_stack = board_state[from_idx]
            if from_stack == 0:
                continue
            for dx, dy in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                to_x = from_x + dx
                to_y = from_y + dy
                if not (0 <= to_x < width and 0 <= to_y < height):
                    continue
                to_idx = to_y * width + to_x
                to_stack = board_state[to_idx]
                if to_stack > 0 and from_stack >= to_stack:
                    moves.append({
                        "from_x": from_x,
                        "from_y": from_y,
                        "to_x": to_x,
                        "to_y": to_y,
                    })
    if not moves:
        return None
    return moves[0]


def main() -> int:
    parser = argparse.ArgumentParser(description="CPU move selector")
    parser.add_argument("--agent_name", type=str, default="AlphaStomper")
    parser.add_argument("--width", type=int, required=True)
    parser.add_argument("--height", type=int, required=True)
    parser.add_argument("--difficulty", choices=("weak", "medium", "strong"), required=True)
    args = parser.parse_args()

    base_dir = Path(__file__).resolve().parent
    payload = json.loads(sys.stdin.read())
    board_state = payload["board_state"]
    model_path = resolve_model_path(base_dir, args.agent_name, args.width, args.height, args.difficulty)
    if model_path is None:
        move = choose_random_move(board_state, args.width, args.height)
    else:
        agent = load_agent(model_path, args.width, args.height)
        move = agent.select_action(board_state, epsilon=0.0)
        if move is None:
            move = choose_random_move(board_state, args.width, args.height)

    if move is None:
        return 2

    print(json.dumps(move, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
