/* Minimal BSD err.h compatibility for the Mosh Windows port. GPL-3.0-or-later. */

#ifndef CNSHELL_MOSH_WINDOWS_ERR_H
#define CNSHELL_MOSH_WINDOWS_ERR_H

#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

inline void cnshell_vwarn(const char *format, va_list arguments, bool include_errno) {
  vfprintf(stderr, format, arguments);
  if (include_errno) {
    fprintf(stderr, ": %s", strerror(errno));
  }
  fputc('\n', stderr);
}

inline void warn(const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  cnshell_vwarn(format, arguments, true);
  va_end(arguments);
}

inline void warnx(const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  cnshell_vwarn(format, arguments, false);
  va_end(arguments);
}

[[noreturn]] inline void err(int status, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  cnshell_vwarn(format, arguments, true);
  va_end(arguments);
  exit(status);
}

[[noreturn]] inline void errx(int status, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  cnshell_vwarn(format, arguments, false);
  va_end(arguments);
  exit(status);
}

#endif
