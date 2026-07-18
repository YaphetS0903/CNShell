#ifndef CNSHELL_MOSH_WINDOWS_PREFIX_H
#define CNSHELL_MOSH_WINDOWS_PREFIX_H

#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <BaseTsd.h>
#include <errno.h>
#include <limits.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <time.h>
#include <wchar.h>

typedef SSIZE_T ssize_t;
typedef int socklen_t;

#ifndef __attribute__
#define __attribute__(value)
#endif
#ifndef __attribute
#define __attribute(value)
#endif
#ifndef STDIN_FILENO
#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2
#endif
#ifndef MSG_DONTWAIT
#define MSG_DONTWAIT 0
#endif
#ifndef EWOULDBLOCK
#define EWOULDBLOCK EAGAIN
#endif
#ifndef EMSGSIZE
#define EMSGSIZE 90
#endif
#ifndef ENOTTY
#define ENOTTY 25
#endif
#ifndef ENOTSUP
#define ENOTSUP 95
#endif
#ifndef SIGWINCH
#define SIGWINCH 28
#endif
#ifndef SIGHUP
#define SIGHUP 1
#endif
#ifndef SIGPIPE
#define SIGPIPE 13
#endif
#ifndef SIGCONT
#define SIGCONT 18
#endif
#ifndef SIGSTOP
#define SIGSTOP 19
#endif

struct termios {
  DWORD input_mode;
};

struct winsize {
  unsigned short ws_row;
  unsigned short ws_col;
  unsigned short ws_xpixel;
  unsigned short ws_ypixel;
};

struct rlimit {
  unsigned long long rlim_cur;
  unsigned long long rlim_max;
};
typedef unsigned long long rlim_t;

#define TCSANOW 0
#define TIOCGWINSZ 0x5413
#define RLIMIT_CORE 4

extern "C" {
ssize_t cnshell_read(int fd, void *buffer, size_t length);
ssize_t cnshell_write(int fd, const void *buffer, size_t length);
int unsetenv(const char *name);
int nanosleep(const struct timespec *request, struct timespec *remaining);
int kill(int process, int signal_number);
int tcgetattr(int fd, struct termios *attributes);
int tcsetattr(int fd, int action, const struct termios *attributes);
void cfmakeraw(struct termios *attributes);
int ioctl(int fd, unsigned long request, void *argument);
int gettimeofday(struct timeval *value, void *timezone);
int getrlimit(int resource, struct rlimit *limit);
int setrlimit(int resource, const struct rlimit *limit);
int wcwidth(wchar_t value);
}

#endif
