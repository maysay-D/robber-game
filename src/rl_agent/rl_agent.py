import random
import torch
import torch.nn as nn


class RLAgent(nn.Module):
    def __init__(self, width, height):
        super().__init__()
        self.width = width
        self.height = height
        self.total_cells = width * height
        self.action_dim = self.total_cells * self.total_cells

        self.network = nn.Sequential(
            nn.Linear(self.total_cells, 128),
            nn.ReLU(),
            nn.Linear(128, 128),
            nn.ReLU(),
            nn.Linear(128, self.action_dim),
        )

    def _prepare_state(self, board_state):
        if isinstance(board_state, torch.Tensor):
            state_tensor = board_state.detach().float()
        else:
            state_tensor = torch.tensor(board_state, dtype=torch.float32)

        if state_tensor.dim() == 1:
            state_tensor = state_tensor.unsqueeze(0)

        if state_tensor.size(-1) != self.total_cells:
            raise ValueError(
                f"Expected input size {self.total_cells}, got {state_tensor.size(-1)}"
            )

        max_value = state_tensor.max(dim=-1, keepdim=True).values.clamp_min(1.0)
        return state_tensor / max_value

    def forward(self, x):
        return self.network(self._prepare_state(x))

    def _board_as_list(self, board_state):
        if isinstance(board_state, torch.Tensor):
            return board_state.detach().cpu().tolist()
        return list(board_state)

    def get_valid_mask(self, board_state):
        board_values = self._board_as_list(board_state)
        valid_mask = torch.zeros(self.action_dim, dtype=torch.bool)

        for from_y in range(self.height):
            for from_x in range(self.width):
                from_idx = from_y * self.width + from_x
                from_stack = board_values[from_idx]

                if from_stack == 0:
                    continue

                for to_y in range(self.height):
                    for to_x in range(self.width):
                        to_idx = to_y * self.width + to_x

                        if from_idx == to_idx:
                            continue
                        if abs(from_x - to_x) + abs(from_y - to_y) != 1:
                            continue

                        to_stack = board_values[to_idx]
                        if to_stack == 0:
                            continue
                        if from_stack < to_stack:
                            continue

                        action_idx = from_idx * self.total_cells + to_idx
                        valid_mask[action_idx] = True

        return valid_mask

    def select_action(self, board_state, epsilon=0.0):
        state_tensor = self._prepare_state(board_state)
        valid_mask = self.get_valid_mask(board_state)

        if not valid_mask.any():
            return None

        if random.random() < epsilon:
            valid_indices = valid_mask.nonzero(as_tuple=True)[0]
            best_action_idx = int(random.choice(valid_indices.tolist()))
        else:
            with torch.no_grad():
                q_values = self(state_tensor)
                if q_values.dim() > 1:
                    q_values = q_values.squeeze(0)

            masked_q_values = q_values.clone()
            masked_q_values[~valid_mask] = -float("inf")
            best_action_idx = int(masked_q_values.argmax().item())

        return {
            "action_idx": best_action_idx,
            "from_x": (best_action_idx // self.total_cells) % self.width,
            "from_y": (best_action_idx // self.total_cells) // self.width,
            "to_x": (best_action_idx % self.total_cells) % self.width,
            "to_y": (best_action_idx % self.total_cells) // self.width,
        }