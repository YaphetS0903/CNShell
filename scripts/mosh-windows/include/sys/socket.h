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

#ifndef MSG_TRUNC
#define MSG_TRUNC 0x0100
#endif
#ifdef CMSG_FIRSTHDR
#undef CMSG_FIRSTHDR
#endif
#ifdef CMSG_DATA
#undef CMSG_DATA
#endif
#define CMSG_FIRSTHDR(message) ((struct cmsghdr *)0)
#define CMSG_DATA(header) ((unsigned char *)((header) + 1))

#endif
