/* Native Windows entry point for the official Mosh 1.4.0 client core. */

#include "config.h"
#include "version.h"
#include "stmclient.h"
#include "crypto.h"
#include "locale_utils.h"
#include "network.h"
#include "timestamp.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <memory>
#include <string>
#include <vector>

namespace {
class WinsockSession {
public:
  WinsockSession() : active(false) {
    WSADATA data{};
    if (WSAStartup(MAKEWORD(2, 2), &data) != 0) {
      throw Network::NetworkException("WSAStartup", WSAGetLastError());
    }
    active = true;
  }
  ~WinsockSession() { if (active) WSACleanup(); }
private:
  bool active;
};

void print_version(FILE *file) {
  fputs("mosh-client (" PACKAGE_STRING ") [build " BUILD_VERSION "]\n"
        "Copyright 2012 Keith Winstein <mosh-devel@mit.edu>\n"
        "Windows adapter Copyright 2026 CNshell contributors\n"
        "License GPLv3+: GNU GPL version 3 or later.\n", file);
}

void print_usage(FILE *file, const char *program) {
  print_version(file);
  fprintf(file, "\nUsage: %s [-# 'ARGS'] IP PORT\n"
                "       %s -c\n"
                "       %s --self-test\n", program, program, program);
}

bool wait_for_payload(Network::Connection &connection, const std::string &expected) {
  ULONGLONG deadline = GetTickCount64() + 3000;
  while (GetTickCount64() < deadline) {
    freeze_timestamp();
    try {
      if (connection.recv() == expected) return true;
    } catch (const Network::NetworkException &) {
    } catch (const Crypto::CryptoException &) {
      return false;
    }
    Sleep(5);
  }
  return false;
}

int self_test() {
  WinsockSession winsock;
  freeze_timestamp();
  Network::Connection server("127.0.0.1", "0");
  const std::string key = server.get_key();
  const std::string port = server.port();
  Network::Connection client(key.c_str(), "127.0.0.1", port.c_str());
  client.send("CNshell Mosh client ping");
  if (!wait_for_payload(server, "CNshell Mosh client ping")) {
    fputs("Mosh Windows UDP receive self-test failed.\n", stderr);
    return 1;
  }
  server.send("CNshell Mosh server pong");
  if (!wait_for_payload(client, "CNshell Mosh server pong")) {
    fputs("Mosh Windows encrypted reply self-test failed.\n", stderr);
    return 1;
  }
  puts("Mosh Windows encrypted UDP loopback passed.");
  return 0;
}

int run_client(int argc, char **argv) {
  unsigned int verbose = 0;
  int positional = 1;

  Crypto::disable_dumping_core();
  if (argc < 1) return 1;
  while (positional < argc) {
    const char *argument = argv[positional];
    if (!strcmp(argument, "--help")) { print_usage(stdout, argv[0]); return 0; }
    if (!strcmp(argument, "--version")) { print_version(stdout); return 0; }
    if (!strcmp(argument, "--self-test")) return self_test();
    if (!strcmp(argument, "-c")) { puts("256"); return 0; }
    if (!strcmp(argument, "-v")) { ++verbose; ++positional; continue; }
    if (!strcmp(argument, "-#")) {
      positional += 2;
      if (positional > argc) { print_usage(stderr, argv[0]); return 1; }
      continue;
    }
    if (argument[0] == '-') { print_usage(stderr, argv[0]); return 1; }
    break;
  }
  if (argc - positional != 2) { print_usage(stderr, argv[0]); return 1; }

  const char *ip = argv[positional];
  const char *port = argv[positional + 1];
  if (!*port || strspn(port, "0123456789") != strlen(port)) {
    fprintf(stderr, "%s: Bad UDP port (%s)\n", argv[0], port);
    return 1;
  }
  const char *environment_key = getenv("MOSH_KEY");
  if (!environment_key || !*environment_key) {
    fputs("MOSH_KEY environment variable not found.\n", stderr);
    return 1;
  }
  const std::string key(environment_key);
  if (unsetenv("MOSH_KEY") < 0) {
    perror("unsetenv");
    return 1;
  }

  set_native_locale();
  WinsockSession winsock;
  bool success = false;
  try {
    STMClient client(ip, port, key.c_str(), getenv("MOSH_PREDICTION_DISPLAY"),
                     verbose, getenv("MOSH_PREDICTION_OVERWRITE"));
    client.init();
    try {
      success = client.main();
    } catch (...) {
      client.shutdown();
      throw;
    }
    client.shutdown();
  } catch (const Network::NetworkException &error) {
    fprintf(stderr, "Network exception: %s\r\n", error.what());
  } catch (const Crypto::CryptoException &error) {
    fprintf(stderr, "Crypto exception: %s\r\n", error.what());
  } catch (const std::exception &error) {
    fprintf(stderr, "Error: %s\r\n", error.what());
  }
  puts("[mosh is exiting.]");
  return success ? 0 : 1;
}

char *wide_to_utf8(const wchar_t *value) {
  int size = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value, -1,
                                 nullptr, 0, nullptr, nullptr);
  if (size <= 0) return nullptr;
  char *result = static_cast<char *>(malloc(static_cast<size_t>(size)));
  if (!result) return nullptr;
  if (!WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value, -1, result,
                           size, nullptr, nullptr)) {
    free(result);
    return nullptr;
  }
  return result;
}
}

int wmain(int argc, wchar_t **wide_argv) {
  std::vector<char *> argv(static_cast<size_t>(argc) + 1, nullptr);
  for (int index = 0; index < argc; ++index) {
    argv[static_cast<size_t>(index)] = wide_to_utf8(wide_argv[index]);
    if (!argv[static_cast<size_t>(index)]) {
      for (char *value : argv) free(value);
      return 1;
    }
  }
  int result = run_client(argc, argv.data());
  for (char *value : argv) free(value);
  return result;
}
