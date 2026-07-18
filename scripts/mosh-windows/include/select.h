#ifndef CNSHELL_MOSH_WINDOWS_SELECT_H
#define CNSHELL_MOSH_WINDOWS_SELECT_H

#include "mosh-windows-compat.h"
#include <algorithm>
#include <assert.h>
#include <vector>

class Select {
public:
  static Select &get_instance(void) {
    static Select instance;
    return instance;
  }

  void add_fd(int descriptor) {
    if (std::find(all_fds.begin(), all_fds.end(), descriptor) == all_fds.end()) {
      all_fds.push_back(descriptor);
    }
  }

  void clear_fds(void) {
    all_fds.clear();
    read_fds.clear();
    stdin_is_ready = false;
  }

  static void add_signal(int signum) {
    if (signum == SIGWINCH) {
      unsigned short columns = 0;
      unsigned short rows = 0;
      Select &instance = get_instance();
      if (cnshell_console_size(&columns, &rows)) {
        instance.last_columns = columns;
        instance.last_rows = rows;
      }
    }
  }

  int select(int timeout) {
    read_fds.assign(all_fds.size(), -1);
    size_t ready_count = 0;
    const bool include_stdin =
        std::find(all_fds.begin(), all_fds.end(), STDIN_FILENO) != all_fds.end();
    int result = cnshell_wait(all_fds.data(), all_fds.size(), include_stdin, timeout,
                              read_fds.data(), read_fds.size(), &stdin_is_ready);
    if (result < 0) {
      read_fds.clear();
      return result;
    }
    ready_count = static_cast<size_t>(result - (stdin_is_ready ? 1 : 0));
    if (ready_count < read_fds.size()) {
      read_fds.resize(ready_count);
    }
    return result;
  }

  bool read(int descriptor) const {
    if (descriptor == STDIN_FILENO) {
      return stdin_is_ready;
    }
    return std::find(read_fds.begin(), read_fds.end(), descriptor) != read_fds.end();
  }

  bool signal(int signum) {
    if (signum != SIGWINCH) {
      return false;
    }
    unsigned short columns = 0;
    unsigned short rows = 0;
    if (!cnshell_console_size(&columns, &rows)) {
      return false;
    }
    const bool changed = columns != last_columns || rows != last_rows;
    last_columns = columns;
    last_rows = rows;
    return changed;
  }

  bool any_signal(void) const { return false; }
  static void set_verbose(unsigned int) {}

private:
  Select()
      : all_fds(), read_fds(), stdin_is_ready(false), last_columns(0), last_rows(0) {}
  Select(const Select &);
  Select &operator=(const Select &);

  std::vector<int> all_fds;
  std::vector<int> read_fds;
  bool stdin_is_ready;
  unsigned short last_columns;
  unsigned short last_rows;
};

#endif
