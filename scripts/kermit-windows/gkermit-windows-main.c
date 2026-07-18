/*
 * CNshell Windows entry point for G-Kermit 2.01.
 * Copyright (C) 2026 CNshell contributors.
 *
 * This file is free software under the GNU General Public License,
 * version 2 or (at your option) any later version.
 */

#include <windows.h>
#include <stdlib.h>

int gkermit_main(int argc, char **argv);

static char *wide_to_utf8(const wchar_t *value) {
    int size;
    char *result;

    size = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value, -1, NULL, 0, NULL, NULL);
    if (size <= 0)
        return NULL;
    result = (char *)malloc((size_t)size);
    if (!result)
        return NULL;
    if (!WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value, -1, result, size, NULL, NULL)) {
        free(result);
        return NULL;
    }
    return result;
}

int wmain(int argc, wchar_t **wide_argv) {
    char **argv;
    int index;

    argv = (char **)calloc((size_t)argc + 1, sizeof(char *));
    if (!argv)
        return 1;
    for (index = 0; index < argc; index++) {
        argv[index] = wide_to_utf8(wide_argv[index]);
        if (!argv[index]) {
            while (index > 0)
                free(argv[--index]);
            free(argv);
            return 1;
        }
    }
    return gkermit_main(argc, argv);
}

