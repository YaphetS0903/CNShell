/* Build the upstream Mosh network transport against the WinSock descriptor adapter. */

#include "config.h"
#include "network.h"
#include "crypto.h"
#include "byteorder.h"
#include "dos_assert.h"
#include "fatal_assert.h"
#include "timestamp.h"
#include "mosh-windows-compat.h"

#define socket cnshell_socket
#define setsockopt cnshell_setsockopt
#define bind cnshell_bind
#define sendto cnshell_sendto
#define recvmsg cnshell_recvmsg
#define getsockname cnshell_getsockname
#define close cnshell_close_socket
#define dup cnshell_dup_socket
#define dup2 cnshell_dup2_socket

#include "network.cc"

#undef socket
#undef setsockopt
#undef bind
#undef sendto
#undef recvmsg
#undef getsockname
#undef close
#undef dup
#undef dup2
