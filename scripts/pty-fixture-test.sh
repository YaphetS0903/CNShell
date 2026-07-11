#!/bin/zsh
set -euo pipefail

ROOT="${0:A:h}/.."
PORT="${CNSHELL_TEST_PTY_PORT:-$(ruby -rsocket -e 'server=TCPServer.new("127.0.0.1",0); puts server.addr[1]; server.close')}"

python3 "$ROOT/scripts/password-ssh-server.py" "$PORT" &
SERVER_PID=$!
cleanup() {
  kill "$SERVER_PID" 2>/dev/null || true
}
trap cleanup EXIT

for _ in {1..50}; do
  nc -z 127.0.0.1 "$PORT" 2>/dev/null && break
  sleep 0.1
done
nc -z 127.0.0.1 "$PORT"

expect <<EOF
set timeout 10
spawn env LANG=en_US.UTF-8 ssh -tt -p $PORT -o PreferredAuthentications=password -o PubkeyAuthentication=no -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null cnshell@127.0.0.1
expect "password:"
send -- "cnshell-test-password\r"
expect "CNshell 本地 PTY 夹具"
expect "cnshell@test:~\\$ "
send -- "中文输入🚀\r"
expect "收到：中文输入🚀"
expect "cnshell@test:~\\$ "
send -- "cnshell-demo\r"
expect "CNshell 全屏 TUI"
expect "中文宽字符：你好，世界"
expect "Emoji：🚀 🟢"
expect "True Color"
send -- "exit\r"
expect "logout"
expect eof
EOF

echo "PTY 夹具通过：密码认证、交互 Shell、中文/Emoji 回显和 ANSI 全屏输出均正常。"
