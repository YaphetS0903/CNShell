#ifndef CNSHELL_MOSH_WINDOWS_SYS_SOCKET_H
#define CNSHELL_MOSH_WINDOWS_SYS_SOCKET_H
#include "mosh-windows-prefix.h"

struct iovec {
  void *iov_base;
  size_t iov_len;
};

struct msghdr {
  void *msg_name;
  socklen_t msg_namelen;
  struct iovec *msg_iov;
  size_t msg_iovlen;
  void *msg_control;
  size_t msg_controllen;
  int msg_flags;
};

struct cmsghdr {
  size_t cmsg_len;
  int cmsg_level;
  int cmsg_type;
};

#ifndef MSG_TRUNC
#define MSG_TRUNC 0x0100
#endif
#define CMSG_FIRSTHDR(message) ((struct cmsghdr *)0)
#define CMSG_DATA(header) ((unsigned char *)((header) + 1))

#endif
