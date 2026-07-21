import copy
import random

import numpy as np
import torch
import torch.nn.functional as F
import torch.optim as optim

from rl_agent import RLAgent

# ==========================================
# ★ここで学習させたい盤面サイズを指定します
# Rustの main.rs の TitleSettings と合わせてください
BOARD_WIDTH = 4
BOARD_HEIGHT = 1
# ==========================================


class MockRobberGameEnv:
    def __init__(self, width=BOARD_WIDTH, height=BOARD_HEIGHT):
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


def count_legal_moves(board, width=BOARD_WIDTH, height=BOARD_HEIGHT):
    count = 0
    total_cells = width * height

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


def compute_reward(board_before, board_after, current_player, winner, done):
    if done:
        return 1.0 if winner == current_player else -1.0

    before_moves = count_legal_moves(board_before)
    after_moves = count_legal_moves(board_after)

    # 局面を縮める方向へ少しだけ報酬を与える
    mobility_delta = before_moves - after_moves
    return 0.02 * mobility_delta


def train():
    random.seed(0)
    np.random.seed(0)
    torch.manual_seed(0)

    env = MockRobberGameEnv(BOARD_WIDTH, BOARD_HEIGHT)
    agent = RLAgent(BOARD_WIDTH, BOARD_HEIGHT)
    optimizer = optim.Adam(agent.parameters(), lr=1e-3)

    target_network = copy.deepcopy(agent.network)
    target_network.eval()
    for p in target_network.parameters():
        p.requires_grad_(False)

    memory = ReplayBuffer(capacity=20000)

    batch_size = 128
    episodes = 6000
    gamma = 0.95

    epsilon_start = 0.35
    epsilon_end = 0.05
    target_update_interval = 200

    print(f"盤面サイズ {BOARD_WIDTH}x{BOARD_HEIGHT} での自己対局を開始します...")

    train_steps = 0
    for episode in range(episodes):
        state, current_player = env.reset()

        epsilon = max(
            epsilon_end,
            epsilon_start - (episode / max(1, episodes - 1)) * (epsilon_start - epsilon_end),
            )

        max_steps = BOARD_WIDTH * BOARD_HEIGHT * 4
        for _ in range(max_steps):
            action_info = agent.select_action(state, epsilon=epsilon)
            if action_info is None:
                break

            action_idx = action_info["action_idx"]
            next_state, winner, done = env.step(action_idx)

            reward = compute_reward(state, next_state, current_player, winner, done)

            # 同一エージェントに対して、自分視点・相手視点の両方から学ぶ
            memory.push(state, action_idx, reward, next_state, done)
            memory.push(state, action_idx, -reward, next_state, done)

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

                target_q = rewards + gamma * (1.0 - dones) * target_q_values

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

    weight_name = f"weights_{BOARD_WIDTH}x{BOARD_HEIGHT}.pth"
    torch.save(agent.state_dict(), weight_name)
    print(f"学習が完了し、モデル '{weight_name}' を保存しました！")


if __name__ == "__main__":
    train()