#ifndef CNSHELL_MOSH_WINDOWS_COMPAT_H
#define CNSHELL_MOSH_WINDOWS_COMPAT_H

#include "mosh-windows-prefix.h"
#include <sys/socket.h>
#include <stddef.h>

int cnshell_socket(int family, int type, int protocol);
int cnshell_setsockopt(int descriptor, int level, int option, const void *value, int length);
int cnshell_bind(int descriptor, const struct sockaddr *address, socklen_t length);
ssize_t cnshell_sendto(int descriptor, const char *buffer, size_t length, int flags,
                       const struct sockaddr *address, socklen_t address_length);
ssize_t cnshell_recvmsg(int descriptor, struct msghdr *message, int flags);
int cnshell_getsockname(int descriptor, struct sockaddr *address, socklen_t *length);
int cnshell_close_socket(int descriptor);
int cnshell_dup_socket(int descriptor);
int cnshell_dup2_socket(int source, int destination);
int cnshell_wait(const int *descriptors, size_t descriptor_count, bool include_stdin,
                 int timeout_ms, int *ready, size_t ready_capacity,
                 bool *stdin_ready);
bool cnshell_console_size(unsigned short *columns, unsigned short *rows);

#endif
