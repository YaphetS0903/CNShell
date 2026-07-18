/* Fixed xterm-256color display capabilities for CNshell's embedded terminal. */

#include "terminaldisplay.h"

using namespace Terminal;

Display::Display(bool use_environment)
    : has_ech(true), has_bce(true), has_title(true),
      smcup(use_environment ? "\033[?1049h" : nullptr),
      rmcup(use_environment ? "\033[?1049l" : nullptr) {}
