import torch

# .pthファイルの読み込み（CPU環境にロードする場合）
data = torch.load('weights_4x1.pth', map_location=torch.device('cpu'))

# 1. 保存されているデータの型や構造（キー）を確認する
print(type(data))
if isinstance(data, dict):
    print(data.keys())

for key, value in data.items():
    print(key, value.size())
