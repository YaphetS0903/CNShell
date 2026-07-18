/* Windows host adapter for the Mosh 1.4.0 protocol core. GPL-3.0-or-later. */

#include "mosh-windows-compat.h"

#include <bcrypt.h>
#include <fcntl.h>
#include <io.h>
#include <algorithm>
#include <array>
#include <cstring>
#include <mutex>

namespace {
constexpr int kFirstDescriptor = 3;
constexpr int kDescriptorCount = 61;
std::array<SOCKET, kDescriptorCount> sockets = [] {
  std::array<SOCKET, kDescriptorCount> result{};
  result.fill(INVALID_SOCKET);
  return result;
}();
std::mutex sockets_mutex;

int set_errno_from_winsock(int code) {
  int mapped = EIO;
  switch (code) {
  case WSAEWOULDBLOCK: mapped = EWOULDBLOCK; break;
  case WSAEINTR: mapped = EINTR; break;
  case WSAEINVAL: mapped = EINVAL; break;
  case WSAEMSGSIZE: mapped = EMSGSIZE; break;
  default: break;
  }
  _set_errno(mapped);
  return mapped;
}

SOCKET lookup_locked(int descriptor) {
  const int index = descriptor - kFirstDescriptor;
  if (index < 0 || index >= kDescriptorCount) {
    return INVALID_SOCKET;
  }
  return sockets[static_cast<size_t>(index)];
}

SOCKET lookup(int descriptor) {
  std::lock_guard<std::mutex> guard(sockets_mutex);
  return lookup_locked(descriptor);
}

int adopt_locked(SOCKET value) {
  for (int index = 0; index < kDescriptorCount; ++index) {
    if (sockets[static_cast<size_t>(index)] == INVALID_SOCKET) {
      sockets[static_cast<size_t>(index)] = value;
      return index + kFirstDescriptor;
    }
  }
  _set_errno(EMFILE);
  return -1;
}

SOCKET duplicate_native(SOCKET source) {
  WSAPROTOCOL_INFOW protocol{};
  if (WSADuplicateSocketW(source, GetCurrentProcessId(), &protocol) != 0) {
    set_errno_from_winsock(WSAGetLastError());
    return INVALID_SOCKET;
  }
  SOCKET duplicate = WSASocketW(FROM_PROTOCOL_INFO, FROM_PROTOCOL_INFO,
                                FROM_PROTOCOL_INFO, &protocol, 0,
                                WSA_FLAG_OVERLAPPED);
  if (duplicate == INVALID_SOCKET) {
    set_errno_from_winsock(WSAGetLastError());
  }
  return duplicate;
}

bool stdin_ready_now() {
  HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
  if (!input || input == INVALID_HANDLE_VALUE) {
    return false;
  }
  if (GetFileType(input) == FILE_TYPE_PIPE) {
    DWORD available = 0;
    return PeekNamedPipe(input, nullptr, 0, nullptr, &available, nullptr) && available > 0;
  }
  return WaitForSingleObject(input, 0) == WAIT_OBJECT_0;
}
}

int cnshell_socket(int family, int type, int protocol) {
  SOCKET value = WSASocketW(family, type, protocol, nullptr, 0, WSA_FLAG_OVERLAPPED);
  if (value == INVALID_SOCKET) {
    set_errno_from_winsock(WSAGetLastError());
    return -1;
  }
  u_long nonblocking = 1;
  if (ioctlsocket(value, FIONBIO, &nonblocking) != 0) {
    set_errno_from_winsock(WSAGetLastError());
    closesocket(value);
    return -1;
  }
  std::lock_guard<std::mutex> guard(sockets_mutex);
  int descriptor = adopt_locked(value);
  if (descriptor < 0) {
    closesocket(value);
  }
  return descriptor;
}

int cnshell_setsockopt(int descriptor, int level, int option, const void *value, int length) {
  SOCKET native = lookup(descriptor);
  if (native == INVALID_SOCKET) { _set_errno(EBADF); return -1; }
  int result = ::setsockopt(native, level, option, static_cast<const char *>(value), length);
  if (result != 0) { set_errno_from_winsock(WSAGetLastError()); return -1; }
  return 0;
}

int cnshell_bind(int descriptor, const struct sockaddr *address, socklen_t length) {
  SOCKET native = lookup(descriptor);
  if (native == INVALID_SOCKET) { _set_errno(EBADF); return -1; }
  int result = ::bind(native, address, length);
  if (result != 0) { set_errno_from_winsock(WSAGetLastError()); return -1; }
  return 0;
}

ssize_t cnshell_sendto(int descriptor, const char *buffer, size_t length, int flags,
                       const struct sockaddr *address, socklen_t address_length) {
  SOCKET native = lookup(descriptor);
  if (native == INVALID_SOCKET) { _set_errno(EBADF); return -1; }
  if (length > INT_MAX) { _set_errno(EMSGSIZE); return -1; }
  int result = ::sendto(native, buffer, static_cast<int>(length), flags, address, address_length);
  if (result == SOCKET_ERROR) { set_errno_from_winsock(WSAGetLastError()); return -1; }
  return result;
}

ssize_t cnshell_recvmsg(int descriptor, struct msghdr *message, int flags) {
  SOCKET native = lookup(descriptor);
  if (native == INVALID_SOCKET || !message || !message->msg_iov || message->msg_iovlen != 1) {
    _set_errno(EINVAL);
    return -1;
  }
  struct iovec &buffer = message->msg_iov[0];
  if (buffer.iov_len > INT_MAX) { _set_errno(EMSGSIZE); return -1; }
  int address_length = message->msg_namelen;
  int result = ::recvfrom(native, static_cast<char *>(buffer.iov_base),
                          static_cast<int>(buffer.iov_len), flags,
                          static_cast<struct sockaddr *>(message->msg_name), &address_length);
  message->msg_namelen = address_length;
  message->msg_controllen = 0;
  message->msg_flags = 0;
  if (result == SOCKET_ERROR) {
    int error = WSAGetLastError();
    if (error == WSAEMSGSIZE) {
      message->msg_flags = MSG_TRUNC;
      return static_cast<ssize_t>(buffer.iov_len);
    }
    set_errno_from_winsock(error);
    return -1;
  }
  return result;
}

int cnshell_getsockname(int descriptor, struct sockaddr *address, socklen_t *length) {
  SOCKET native = lookup(descriptor);
  if (native == INVALID_SOCKET) { _set_errno(EBADF); return -1; }
  int result = ::getsockname(native, address, length);
  if (result != 0) { set_errno_from_winsock(WSAGetLastError()); return -1; }
  return 0;
}

int cnshell_close_socket(int descriptor) {
  std::lock_guard<std::mutex> guard(sockets_mutex);
  SOCKET value = lookup_locked(descriptor);
  if (value == INVALID_SOCKET) { _set_errno(EBADF); return -1; }
  sockets[static_cast<size_t>(descriptor - kFirstDescriptor)] = INVALID_SOCKET;
  if (closesocket(value) != 0) { set_errno_from_winsock(WSAGetLastError()); return -1; }
  return 0;
}

int cnshell_dup_socket(int descriptor) {
  std::lock_guard<std::mutex> guard(sockets_mutex);
  SOCKET source = lookup_locked(descriptor);
  if (source == INVALID_SOCKET) { _set_errno(EBADF); return -1; }
  SOCKET duplicate = duplicate_native(source);
  if (duplicate == INVALID_SOCKET) return -1;
  int result = adopt_locked(duplicate);
  if (result < 0) closesocket(duplicate);
  return result;
}

int cnshell_dup2_socket(int source_descriptor, int destination_descriptor) {
  std::lock_guard<std::mutex> guard(sockets_mutex);
  SOCKET source = lookup_locked(source_descriptor);
  int destination_index = destination_descriptor - kFirstDescriptor;
  if (source == INVALID_SOCKET || destination_index < 0 || destination_index >= kDescriptorCount) {
    _set_errno(EBADF);
    return -1;
  }
  SOCKET duplicate = duplicate_native(source);
  if (duplicate == INVALID_SOCKET) return -1;
  SOCKET old = sockets[static_cast<size_t>(destination_index)];
  sockets[static_cast<size_t>(destination_index)] = duplicate;
  if (old != INVALID_SOCKET) closesocket(old);
  return destination_descriptor;
}

int cnshell_wait(const int *descriptors, size_t descriptor_count, bool include_stdin,
                 int timeout_ms, int *ready, size_t ready_capacity, bool *stdin_ready) {
  const ULONGLONG started = GetTickCount64();
  if (stdin_ready) *stdin_ready = false;
  for (;;) {
    fd_set readable;
    FD_ZERO(&readable);
    std::array<int, FD_SETSIZE> ids{};
    size_t id_count = 0;
    for (size_t index = 0; index < descriptor_count && id_count < ids.size(); ++index) {
      int descriptor = descriptors[index];
      if (descriptor == STDIN_FILENO) continue;
      SOCKET native = lookup(descriptor);
      if (native != INVALID_SOCKET) {
        FD_SET(native, &readable);
        ids[id_count++] = descriptor;
      }
    }
    timeval poll{0, 0};
    int socket_ready = id_count ? ::select(0, &readable, nullptr, nullptr, &poll) : 0;
    if (socket_ready == SOCKET_ERROR) {
      set_errno_from_winsock(WSAGetLastError());
      return -1;
    }
    size_t ready_count = 0;
    for (size_t index = 0; index < id_count && ready_count < ready_capacity; ++index) {
      SOCKET native = lookup(ids[index]);
      if (native != INVALID_SOCKET && FD_ISSET(native, &readable)) {
        ready[ready_count++] = ids[index];
      }
    }
    bool input_ready = include_stdin && stdin_ready_now();
    if (stdin_ready) *stdin_ready = input_ready;
    if (ready_count > 0 || input_ready) {
      return static_cast<int>(ready_count + (input_ready ? 1 : 0));
    }
    if (timeout_ms == 0) return 0;
    if (timeout_ms > 0 && GetTickCount64() - started >= static_cast<ULONGLONG>(timeout_ms)) {
      return 0;
    }
    DWORD delay = 10;
    if (timeout_ms > 0) {
      ULONGLONG elapsed = GetTickCount64() - started;
      ULONGLONG remaining = static_cast<ULONGLONG>(timeout_ms) - std::min<ULONGLONG>(elapsed, timeout_ms);
      delay = static_cast<DWORD>(std::min<ULONGLONG>(delay, remaining));
    }
    Sleep(delay ? delay : 1);
  }
}

bool cnshell_console_size(unsigned short *columns, unsigned short *rows) {
  CONSOLE_SCREEN_BUFFER_INFO info{};
  HANDLE output = GetStdHandle(STD_OUTPUT_HANDLE);
  if (!columns || !rows || !output || output == INVALID_HANDLE_VALUE ||
      !GetConsoleScreenBufferInfo(output, &info)) {
    return false;
  }
  int width = info.srWindow.Right - info.srWindow.Left + 1;
  int height = info.srWindow.Bottom - info.srWindow.Top + 1;
  if (width < 1 || height < 1 || width > USHRT_MAX || height > USHRT_MAX) return false;
  *columns = static_cast<unsigned short>(width);
  *rows = static_cast<unsigned short>(height);
  return true;
}

extern "C" ssize_t cnshell_read(int fd, void *buffer, size_t length) {
  if (fd != STDIN_FILENO || !buffer || length > MAXDWORD) { _set_errno(EINVAL); return -1; }
  HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
  DWORD received = 0;
  if (!ReadFile(input, buffer, static_cast<DWORD>(length), &received, nullptr)) {
    DWORD error = GetLastError();
    if (error == ERROR_BROKEN_PIPE || error == ERROR_HANDLE_EOF) return 0;
    _set_errno(EIO);
    return -1;
  }
  return static_cast<ssize_t>(received);
}

extern "C" ssize_t cnshell_write(int fd, const void *buffer, size_t length) {
  if ((fd != STDOUT_FILENO && fd != STDERR_FILENO) || !buffer || length > MAXDWORD) {
    _set_errno(EINVAL);
    return -1;
  }
  HANDLE output = GetStdHandle(fd == STDOUT_FILENO ? STD_OUTPUT_HANDLE : STD_ERROR_HANDLE);
  DWORD written = 0;
  if (!WriteFile(output, buffer, static_cast<DWORD>(length), &written, nullptr)) {
    _set_errno(EIO);
    return -1;
  }
  return static_cast<ssize_t>(written);
}

extern "C" int unsetenv(const char *name) {
  if (!name || strchr(name, '=') != nullptr) { _set_errno(EINVAL); return -1; }
  return _putenv_s(name, "") == 0 ? 0 : -1;
}

extern "C" int nanosleep(const struct timespec *request, struct timespec *remaining) {
  if (!request || request->tv_sec < 0 || request->tv_nsec < 0 || request->tv_nsec >= 1000000000L) {
    _set_errno(EINVAL);
    return -1;
  }
  unsigned long long milliseconds = static_cast<unsigned long long>(request->tv_sec) * 1000ULL +
                                    static_cast<unsigned long long>(request->tv_nsec) / 1000000ULL;
  Sleep(static_cast<DWORD>(std::min<unsigned long long>(milliseconds, MAXDWORD)));
  if (remaining) { remaining->tv_sec = 0; remaining->tv_nsec = 0; }
  return 0;
}

extern "C" int kill(int, int) { _set_errno(ENOTSUP); return -1; }

extern "C" int tcgetattr(int fd, struct termios *attributes) {
  if (fd != STDIN_FILENO || !attributes || !GetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), &attributes->input_mode)) {
    _set_errno(ENOTTY);
    return -1;
  }
  return 0;
}

extern "C" void cfmakeraw(struct termios *attributes) {
  if (!attributes) return;
  attributes->input_mode &= ~(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT);
  attributes->input_mode |= ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_EXTENDED_FLAGS;
}

extern "C" int tcsetattr(int fd, int, const struct termios *attributes) {
  if (fd != STDIN_FILENO || !attributes ||
      !SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), attributes->input_mode)) {
    _set_errno(ENOTTY);
    return -1;
  }
  return 0;
}

extern "C" int ioctl(int fd, unsigned long request, void *argument) {
  if (fd != STDIN_FILENO || request != TIOCGWINSZ || !argument) { _set_errno(EINVAL); return -1; }
  struct winsize *size = static_cast<struct winsize *>(argument);
  if (!cnshell_console_size(&size->ws_col, &size->ws_row)) { _set_errno(ENOTTY); return -1; }
  size->ws_xpixel = 0;
  size->ws_ypixel = 0;
  return 0;
}

extern "C" int gettimeofday(struct timeval *value, void *) {
  if (!value) { _set_errno(EINVAL); return -1; }
  ULONGLONG milliseconds = GetTickCount64();
  value->tv_sec = static_cast<long>(milliseconds / 1000ULL);
  value->tv_usec = static_cast<long>((milliseconds % 1000ULL) * 1000ULL);
  return 0;
}

extern "C" int getrlimit(int, struct rlimit *limit) {
  if (!limit) { _set_errno(EINVAL); return -1; }
  limit->rlim_cur = 0;
  limit->rlim_max = 0;
  return 0;
}

extern "C" int setrlimit(int, const struct rlimit *) { return 0; }

extern "C" int wcwidth(wchar_t value) {
  if (value == 0) return 0;
  if (value < 0x20 || (value >= 0x7f && value < 0xa0) ||
      (value >= 0xd800 && value <= 0xdfff)) return -1;
  WORD type = 0;
  if (!GetStringTypeW(CT_CTYPE3, &value, 1, &type)) return 1;
  if (type & (C3_NONSPACING | C3_DIACRITIC)) return 0;
  if (type & C3_FULLWIDTH) return 2;
  return 1;
}
