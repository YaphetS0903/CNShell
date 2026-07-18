/* Windows UTF-8 locale adapter for Mosh 1.4.0. GPL-3.0-or-later. */

#include "locale_utils.h"
#include <locale.h>
#include <stdlib.h>

const std::string LocaleVar::str(void) const {
  return name.empty() ? std::string("[Windows UTF-8 locale]") : name + "=" + value;
}

const LocaleVar get_ctype(void) {
  return LocaleVar("LC_CTYPE", ".UTF-8");
}

const char *locale_charset(void) {
  return "UTF-8";
}

bool is_utf8_locale(void) {
  return true;
}

void set_native_locale(void) {
  setlocale(LC_ALL, ".UTF-8");
  SetConsoleCP(CP_UTF8);
  SetConsoleOutputCP(CP_UTF8);
}

void clear_locale_variables(void) {
  static const char *variables[] = {
      "LANG", "LANGUAGE", "LC_CTYPE", "LC_NUMERIC", "LC_TIME", "LC_COLLATE",
      "LC_MONETARY", "LC_MESSAGES", "LC_PAPER", "LC_NAME", "LC_ADDRESS",
      "LC_TELEPHONE", "LC_MEASUREMENT", "LC_IDENTIFICATION", "LC_ALL"};
  for (const char *variable : variables) {
    unsetenv(variable);
  }
}
