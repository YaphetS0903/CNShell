#!/usr/bin/env python3
import socket
import sys
import threading
import logging

import paramiko

logging.getLogger("paramiko").setLevel(logging.CRITICAL)


class PasswordServer(paramiko.ServerInterface):
    def check_auth_password(self, username, password):
        if username == "cnshell" and password == "cnshell-test-password":
            return paramiko.AUTH_SUCCESSFUL
        return paramiko.AUTH_FAILED

    def get_allowed_auths(self, username):
        return "password"

    def check_channel_request(self, kind, chanid):
        return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

    def check_channel_exec_request(self, channel, command):
        def respond():
            channel.send(b"cnshell-password-ok")
            channel.send_exit_status(0)
            channel.close()

        threading.Thread(target=respond, daemon=True).start()
        return True

    def check_channel_pty_request(self, channel, term, width, height, pixelwidth, pixelheight, modes):
        return True

    def check_channel_shell_request(self, channel):
        def interactive():
            channel.send("CNshell 本地 PTY 夹具 · UTF-8 ✓\r\ncnshell@test:~$ ".encode())
            pending = bytearray()
            while True:
                data = channel.recv(4096)
                if not data:
                    break
                channel.send(data)
                pending.extend(data)
                if b"\r" not in pending and b"\n" not in pending:
                    continue
                command = bytes(pending).replace(b"\r", b"").replace(b"\n", b"").decode("utf-8", "replace").strip()
                pending.clear()
                channel.send(b"\r\n")
                if command == "exit":
                    channel.send(b"logout\r\n")
                    channel.close()
                    return
                if command == "cnshell-demo":
                    channel.send(
                        "\x1b[2J\x1b[H"
                        "┌──────────── CNshell 全屏 TUI ────────────┐\r\n"
                        "│ 中文宽字符：你好，世界                  │\r\n"
                        "│ Emoji：🚀 🟢  True Color：\x1b[38;2;70;180;255mRGB\x1b[0m │\r\n"
                        "│ Bracketed/ANSI/光标定位：正常           │\r\n"
                        "└─────────────────────────────────────────┘\r\n"
                        "按键输入回显验证完成\r\n"
                        "cnshell@test:~$ ".encode()
                    )
                else:
                    channel.send("收到：{}\r\ncnshell@test:~$ ".format(command).encode())

        threading.Thread(target=interactive, daemon=True).start()
        return True


def serve(client, host_key):
    transport = paramiko.Transport(client)
    try:
        transport.add_server_key(host_key)
        transport.start_server(server=PasswordServer())
        channel = transport.accept(10)
        if channel is not None:
            while transport.is_active():
                threading.Event().wait(0.05)
    except Exception:
        pass
    finally:
        transport.close()


def main():
    port = int(sys.argv[1])
    host_key = paramiko.RSAKey.generate(2048)
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", port))
    listener.listen(20)
    while True:
        client, _ = listener.accept()
        threading.Thread(target=serve, args=(client, host_key), daemon=True).start()


if __name__ == "__main__":
    main()
