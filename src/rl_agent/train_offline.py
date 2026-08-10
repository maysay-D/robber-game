import copy
import random
import re
import argparse
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F
import torch.optim as optim

from rl_agent import RLAgent

CHECKPOINT_RE = re.compile(
    r"^(?P<agent_name>.+)_(?P<width>\d+)x(?P<height>\d+)_(?P<episodes>\d+)\.pkl$"
)


class MockRobberGameEnv:
    def __init__(self, width, height):
        self.width = max(width, 1)
        self.height = max(height, 1)
        self.total_cells = self.width * self.height
        self.reset()

    def reset(self):
        self.board = [1] * self.total_cells
        self.turn_count = 0
        self.current_player = 1
        return self.board.copy(), self.current_player

    def _index(self, x, y):
        return y * self.width + x

    def can_move(self, from_x, from_y, to_x, to_y):
        if not (0 <= from_x < self.width and 0 <= from_y < self.height):
            return False
        if not (0 <= to_x < self.width and 0 <= to_y < self.height):
            return False
        if from_x == to_x and from_y == to_y:
            return False
        if abs(from_x - to_x) + abs(from_y - to_y) != 1:
            return False

        from_stack = self.board[self._index(from_x, from_y)]
        to_stack = self.board[self._index(to_x, to_y)]
        return from_stack > 0 and to_stack > 0 and from_stack >= to_stack

    def has_legal_move(self):
        for from_y in range(self.height):
            for from_x in range(self.width):
                if self.board[self._index(from_x, from_y)] == 0:
                    continue
                for dx, dy in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    to_x, to_y = from_x + dx, from_y + dy
                    if self.can_move(from_x, from_y, to_x, to_y):
                        return True
        return False

    def step(self, action_idx):
        from_idx = action_idx // self.total_cells
        to_idx = action_idx % self.total_cells
        from_x, from_y = from_idx % self.width, from_idx // self.width
        to_x, to_y = to_idx % self.width, to_idx // self.width

        if self.can_move(from_x, from_y, to_x, to_y):
            moving = self.board[from_idx]
            self.board[from_idx] = 0
            self.board[to_idx] += moving
        else:
            return self.board.copy(), self.current_player * -1, True

        self.turn_count += 1
        done = not self.has_legal_move()
        winner = self.current_player if done else 0
        if not done:
            self.current_player *= -1

        return self.board.copy(), winner, done


class ReplayBuffer:
    def __init__(self, capacity, alpha=0.6):
        self.capacity = capacity
        self.alpha = alpha
        self.buffer = []
        self.priorities = []
        self.position = 0

    def push(self, state, action, reward, next_state, done):
        experience = (state, action, reward, next_state, done)
        if len(self.buffer) < self.capacity:
            self.buffer.append(experience)
            self.priorities.append(1.0)
        else:
            idx = self.position % self.capacity
            self.buffer[idx] = experience
            self.priorities[idx] = 1.0
        self.position += 1

    def sample(self, batch_size, beta=0.4):
        if len(self.buffer) < batch_size:
            return None

        probs = np.array(self.priorities, dtype=np.float64)
        probs = probs ** self.alpha
        probs = probs / probs.sum()

        indices = np.random.choice(len(self.buffer), size=batch_size, replace=True, p=probs)
        weights = (len(self.buffer) * probs[indices]) ** (-beta)
        weights = weights / weights.max()

        batch = [self.buffer[idx] for idx in indices]
        return batch, indices, weights

    def update_priorities(self, indices, td_errors):
        for idx, td_error in zip(indices, td_errors):
            priority = min(1e6, abs(float(td_error)) + 1e-6)
            self.priorities[idx] = priority

    def __len__(self):
        return len(self.buffer)


def count_legal_moves(board, width, height):
    count = 0

    for from_y in range(height):
        for from_x in range(width):
            from_idx = from_y * width + from_x
            from_stack = board[from_idx]

            if from_stack == 0:
                continue

            for to_y in range(height):
                for to_x in range(width):
                    to_idx = to_y * width + to_x

                    if from_idx == to_idx:
                        continue
                    if abs(from_x - to_x) + abs(from_y - to_y) != 1:
                        continue

                    to_stack = board[to_idx]
                    if to_stack == 0:
                        continue
                    if from_stack < to_stack:
                        continue

                    count += 1

    return count


def compute_reward(board_before, board_after, current_player, winner, done, width, height):
    if done:
        return 1.0 if winner == current_player else -1.0

    before_moves = count_legal_moves(board_before, width, height)
    after_moves = count_legal_moves(board_after, width, height)

    # 局面を縮める方向へ少しだけ報酬を与える
    mobility_delta = before_moves - after_moves
    return 0.02 * mobility_delta


def find_latest_checkpoint(base_dir, agent_name, width, height):
    candidates = []
    pattern = f"{agent_name}_{width}x{height}_*.pkl"
    for path in base_dir.glob(pattern):
        match = CHECKPOINT_RE.match(path.name)
        if not match:
            continue
        candidates.append((int(match.group("episodes")), path))
    if not candidates:
        return None
    candidates.sort(key=lambda item: item[0])
    return candidates[-1][1]


def load_checkpoint(path, width, height):
    checkpoint = torch.load(path, map_location="cpu", weights_only=False)
    agent = RLAgent(width, height)
    if isinstance(checkpoint, dict) and "model_state_dict" in checkpoint:
        agent.load_state_dict(checkpoint["model_state_dict"])
        metadata = checkpoint
    else:
        agent.load_state_dict(checkpoint)
        metadata = {}
    agent.eval()
    return agent, metadata


def save_checkpoint(
    path,
    agent,
    agent_name,
    width,
    height,
    episodes,
    policy_table=None,
    is_perfect_agent=False,
):
    torch.save(
        {
            "model_state_dict": agent.state_dict(),
            "board_width": width,
            "board_height": height,
            "agent_name": agent_name,
            "episodes": episodes,
            "policy_table": policy_table,
            "is_perfect_agent": is_perfect_agent,
        },
        path,
    )


def build_policy_table(agent, width, height):
    board_state = [1] * (width * height)
    state_tensor = torch.tensor(board_state, dtype=torch.float32)
    valid_mask = agent.get_valid_mask(board_state)

    with torch.no_grad():
        q_values = agent(state_tensor)
        if q_values.dim() > 1:
            q_values = q_values.squeeze(0)

    masked_q = q_values.clone()
    masked_q[~valid_mask] = -float("inf")
    probabilities = torch.softmax(masked_q[valid_mask], dim=0)

    full_probabilities = [0.0] * (width * height * width * height)
    valid_indices = valid_mask.nonzero(as_tuple=True)[0].tolist()
    for index, probability in zip(valid_indices, probabilities.tolist()):
        full_probabilities[index] = probability

    best_action_idx = int(masked_q.argmax().item())
    best_probability = full_probabilities[best_action_idx]
    return {
        "board_state": board_state,
        "probabilities": full_probabilities,
        "best_action_idx": best_action_idx,
        "best_probability": best_probability,
    }


def checkpoint_is_perfect(checkpoint, threshold=0.999999):
    policy_table = checkpoint.get("policy_table")
    if not isinstance(policy_table, dict):
        return False
    best_probability = float(policy_table.get("best_probability", 0.0))
    return best_probability >= threshold


def persist_policy_table(path, checkpoint, policy_table):
    checkpoint = dict(checkpoint)
    checkpoint["policy_table"] = policy_table
    checkpoint["is_perfect_agent"] = checkpoint_is_perfect(checkpoint)
    torch.save(checkpoint, path)


def ensure_policy_table(path, agent, width, height, checkpoint):
    updated = dict(checkpoint)
    needs_save = False
    if not isinstance(updated.get("policy_table"), dict):
        updated["policy_table"] = build_policy_table(agent, width, height)
        needs_save = True
    updated["is_perfect_agent"] = checkpoint_is_perfect(updated)
    if updated.get("is_perfect_agent") != checkpoint.get("is_perfect_agent") or needs_save:
        torch.save(updated, path)
    return updated


def train():
    parser = argparse.ArgumentParser(description="Train a robber-game RL agent")
    parser.add_argument("--agent_name", type=str, default="AlphaStomper")
    parser.add_argument("--width", type=int, required=True)
    parser.add_argument("--height", type=int, required=True)
    parser.add_argument("--episodes", type=int, required=True, help="additional training episodes")
    args = parser.parse_args()

    random.seed(0)
    np.random.seed(0)
    torch.manual_seed(0)

    width = max(args.width, 1)
    height = max(args.height, 1)
    env = MockRobberGameEnv(width, height)

    base_dir = Path(__file__).resolve().parent
    latest_checkpoint = find_latest_checkpoint(base_dir, args.agent_name, width, height)
    start_episodes = 0
    agent = RLAgent(width, height)
    if latest_checkpoint is not None:
        print(f"既存モデル {latest_checkpoint.name} を読み込みます...")
        agent, metadata = load_checkpoint(latest_checkpoint, width, height)
        metadata = ensure_policy_table(latest_checkpoint, agent, width, height, metadata)
        start_episodes = int(metadata.get("episodes", 0))
        if metadata.get("is_perfect_agent", False):
            print("このモデルは完全解析済みのため、追加学習を終了します。")
            return
    else:
        print("新規学習を開始します。")

    optimizer = optim.Adam(agent.parameters(), lr=1e-3)

    target_network = copy.deepcopy(agent.network)
    target_network.eval()
    for p in target_network.parameters():
        p.requires_grad_(False)

    memory = ReplayBuffer(capacity=20000)

    batch_size = 128
    episodes = args.episodes
    gamma = 0.95

    epsilon_start = 0.35
    epsilon_end = 0.05
    target_update_interval = 200

    print(f"盤面サイズ {width}x{height} で {episodes} エピソードの追加学習を開始します...")

    train_steps = 0
    for episode in range(episodes):
        state, current_player = env.reset()

        epsilon = max(
            epsilon_end,
            epsilon_start - (episode / max(1, episodes - 1)) * (epsilon_start - epsilon_end),
            )

        max_steps = width * height * 4
        for _ in range(max_steps):
            action_info = agent.select_action(state, epsilon=epsilon)
            if action_info is None:
                break

            action_idx = action_info["action_idx"]
            next_state, winner, done = env.step(action_idx)

            reward = compute_reward(state, next_state, current_player, winner, done, width, height)

            # 同一エージェントに対して、自分視点・相手視点の両方から学ぶ
            memory.push(state, action_idx, reward, next_state, done)

            state = next_state
            current_player = env.current_player

            if done:
                break

        if len(memory) >= batch_size:
            batch, indices, sample_weights = memory.sample(batch_size, beta=0.4)

            states = torch.tensor([item[0] for item in batch], dtype=torch.float32)
            actions = torch.tensor([item[1] for item in batch], dtype=torch.int64).unsqueeze(1)
            rewards = torch.tensor([item[2] for item in batch], dtype=torch.float32).unsqueeze(1)
            next_states = torch.tensor([item[3] for item in batch], dtype=torch.float32)
            dones = torch.tensor([item[4] for item in batch], dtype=torch.float32).unsqueeze(1)
            sample_weights = torch.tensor(sample_weights, dtype=torch.float32).unsqueeze(1)

            q_values = agent(states)
            current_q = q_values.gather(1, actions)

            with torch.no_grad():
                next_q_online = agent(next_states)
                next_q_target = target_network(next_states)

                target_q_values = torch.zeros(batch_size, 1, dtype=torch.float32)
                for i in range(batch_size):
                    mask = agent.get_valid_mask(next_states[i].tolist())
                    if not mask.any():
                        continue

                    next_q_online_i = next_q_online[i].clone()
                    next_q_online_i[~mask] = -float("inf")
                    best_action_idx = int(next_q_online_i.argmax().item())
                    target_q_values[i, 0] = next_q_target[i, best_action_idx]

                target_q = rewards - gamma * (1.0 - dones) * target_q_values

            loss = F.smooth_l1_loss(current_q, target_q)
            loss = (sample_weights * loss).mean()

            optimizer.zero_grad()
            loss.backward()
            torch.nn.utils.clip_grad_norm_(agent.parameters(), 5.0)
            optimizer.step()

            train_steps += 1
            if train_steps % target_update_interval == 0:
                target_network.load_state_dict(agent.network.state_dict())

        if (episode + 1) % 100 == 0:
            print(f"Episode {episode + 1}/{episodes} 完了 | epsilon={epsilon:.3f}")

    total_episodes = start_episodes + episodes
    save_path = base_dir / f"{args.agent_name}_{width}x{height}_{total_episodes}.pkl"
    policy_table = build_policy_table(agent, width, height)
    is_perfect_agent = checkpoint_is_perfect({"policy_table": policy_table})
    save_checkpoint(
        save_path,
        agent,
        args.agent_name,
        width,
        height,
        total_episodes,
        policy_table=policy_table,
        is_perfect_agent=is_perfect_agent,
    )
    print(f"モデルを保存しました: {save_path}")


if __name__ == "__main__":
    train()