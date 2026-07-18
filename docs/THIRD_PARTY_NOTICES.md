# CNshell 第三方组件声明

CNshell 随应用分发以下开源组件的目标代码。完整许可证文本位于应用资源目录中的
`freerdp/licenses`、`mosh/licenses`、`kermit/licenses` 与 `licenses` 目录；
macOS 对应 `.app/Contents/Resources`，Windows 对应安装目录中的资源目录。

| 组件 | 版本 | 许可证 | 项目地址 |
| --- | --- | --- | --- |
| FreeRDP | 3.28.0 | Apache-2.0 | <https://github.com/FreeRDP/FreeRDP> |
| OpenSSL | 3.6.3 | Apache-2.0 | <https://github.com/openssl/openssl> |
| SDL | 3.4.12 | Zlib | <https://github.com/libsdl-org/SDL> |
| SDL_ttf | 3.2.2 | Zlib | <https://github.com/libsdl-org/SDL_ttf> |
| FreeType | SDL_ttf 3.2.2 vendored revision | FreeType License | <https://freetype.org> |
| Mosh | 1.4.0 | GPL-3.0-or-later | <https://mosh.org> |
| Protocol Buffers | 21.12 | BSD-3-Clause | <https://github.com/protocolbuffers/protobuf> |
| zlib | 1.3.2 (vcpkg port revision 1) | Zlib | <https://www.zlib.net> |
| serialport-rs | 4.9.0 | MPL-2.0 | <https://github.com/serialport/serialport-rs> |
| G-Kermit | 2.01 | GPL-2.0 | <https://www.kermitproject.org/gkermit.html> |
| Wasmi | 1.1.0 | MIT OR Apache-2.0 | <https://github.com/wasmi-labs/wasmi> |
| ed25519-dalek | 2.2.0 | BSD-3-Clause | <https://github.com/dalek-cryptography/curve25519-dalek> |
| serde_jcs | 0.2.0 | MIT OR Apache-2.0 | <https://github.com/l1h3r/serde_jcs> |
| x25519-dalek | 2.0.1 | BSD-3-Clause | <https://github.com/dalek-cryptography/curve25519-dalek> |
| HKDF | 0.12.4 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/KDFs> |

这些组件按各自许可证以“原样”提供，其作者不对 CNshell 作任何担保或背书。
G-Kermit 的完整对应源码归档随应用位于
`Contents/Resources/kermit/source/gku201.tar.gz`，其 SHA-256 为
`19f9ac00d7b230d0a841928a25676269363c2925afc23e62704cde516fc1abbd`。
Windows 版本使用的 GPL 外部协议适配层与构建脚本位于
`kermit/source/windows-port`，并在 GitHub Release 中单独提供
`gkermit-windows-port-source.zip`。
Mosh 1.4.0 与 Protocol Buffers 21.12 的固定哈希源码归档随应用位于
`mosh/source`；Windows 原生 WinSock/ConPTY 适配层位于
`mosh/source/windows-port`，并在 GitHub Release 中提供对应源码归档。
