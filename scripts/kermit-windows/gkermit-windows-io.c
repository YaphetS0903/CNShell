/*
 * Restricted Windows I/O adapter for G-Kermit 2.01 external protocol mode.
 * Copyright (C) 2026 CNshell contributors.
 *
 * This file is free software under the GNU General Public License,
 * version 2 or (at your option) any later version.
 */

#include <windows.h>
#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <io.h>
#include <limits.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <wchar.h>

#include "gkermit.h"

extern int debug;
extern int keep;
extern int literal;
extern int nomodes;
extern int quiet;
extern int streamok;
extern FILE *db;

FILE *ifp = NULL;
FILE *ofp = NULL;
char zinbuf[MAXRECORD + 1];
int zincnt = 0;
char *zinptr = NULL;

static int xparity = 0;
static char ofile[MAXPATHLEN + 1];
static char work[MAXPATHLEN + 1];

static int path_to_wide(const char *path, wchar_t *result, size_t capacity) {
    wchar_t converted[MAXPATHLEN + 1];
    int count;
    size_t length;

    if (!path || !result || capacity < 8)
        return 0;
    count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, converted,
                                (int)(sizeof(converted) / sizeof(converted[0])));
    if (count <= 0)
        return 0;
    length = (size_t)count;
    if (length >= 4 && converted[0] == L'\\' && converted[1] == L'\\' &&
        converted[2] != L'?' && converted[2] != L'.') {
        if (length + 6 > capacity)
            return 0;
        wcscpy_s(result, capacity, L"\\\\?\\UNC\\");
        wcscat_s(result, capacity, converted + 2);
    } else if (length >= 4 && converted[1] == L':' &&
               (converted[2] == L'\\' || converted[2] == L'/')) {
        if (length + 4 > capacity)
            return 0;
        wcscpy_s(result, capacity, L"\\\\?\\");
        wcscat_s(result, capacity, converted);
    } else {
        if (length > capacity)
            return 0;
        wcscpy_s(result, capacity, converted);
    }
    for (length = 0; result[length]; length++) {
        if (result[length] == L'/')
            result[length] = L'\\';
    }
    return 1;
}

static int path_stat(const char *path, struct _stat64 *metadata) {
    wchar_t wide[MAXPATHLEN + 8];
    if (!path_to_wide(path, wide, sizeof(wide) / sizeof(wide[0]))) {
        errno = EINVAL;
        return -1;
    }
    return _wstat64(wide, metadata);
}

static int path_access(const char *path, int mode) {
    wchar_t wide[MAXPATHLEN + 8];
    if (!path_to_wide(path, wide, sizeof(wide) / sizeof(wide[0]))) {
        errno = EINVAL;
        return -1;
    }
    return _waccess(wide, mode);
}

static FILE *path_open(const char *path, const wchar_t *mode) {
    wchar_t wide[MAXPATHLEN + 8];
    FILE *file = NULL;
    if (!path_to_wide(path, wide, sizeof(wide) / sizeof(wide[0]))) {
        errno = EINVAL;
        return NULL;
    }
    if (_wfopen_s(&file, wide, mode) != 0)
        return NULL;
    return file;
}

static int path_unlink(const char *path) {
    wchar_t wide[MAXPATHLEN + 8];
    if (!path_to_wide(path, wide, sizeof(wide) / sizeof(wide[0]))) {
        errno = EINVAL;
        return -1;
    }
    return _wunlink(wide);
}

static int path_rename(const char *source, const char *destination) {
    wchar_t wide_source[MAXPATHLEN + 8];
    wchar_t wide_destination[MAXPATHLEN + 8];
    if (!path_to_wide(source, wide_source, sizeof(wide_source) / sizeof(wide_source[0])) ||
        !path_to_wide(destination, wide_destination,
                      sizeof(wide_destination) / sizeof(wide_destination[0]))) {
        errno = EINVAL;
        return -1;
    }
    return _wrename(wide_source, wide_destination);
}

static int read_byte(char *value, ULONGLONG deadline) {
    HANDLE input = GetStdHandle(STD_INPUT_HANDLE);

    if (!input || input == INVALID_HANDLE_VALUE)
        return -2;
    for (;;) {
        DWORD available = 0;
        DWORD received = 0;
        if (!PeekNamedPipe(input, NULL, 0, NULL, &available, NULL)) {
            DWORD error = GetLastError();
            if (error == ERROR_BROKEN_PIPE || error == ERROR_PIPE_NOT_CONNECTED)
                return -2;
            return -2;
        }
        if (available > 0) {
            if (!ReadFile(input, value, 1, &received, NULL) || received != 1)
                return -2;
            return 1;
        }
        if (deadline && GetTickCount64() >= deadline)
            return -1;
        Sleep(5);
    }
}

unsigned int gkermit_windows_sleep(unsigned int seconds) {
    Sleep(seconds > UINT_MAX / 1000U ? UINT_MAX : seconds * 1000U);
    return 0;
}

SIGTYP doexit(int status) {
    ttres();
    if (debug && db) {
        fprintf(db, "exit %d\n", status);
        fclose(db);
    }
    exit(status);
    SIGRETURN;
}

VOID sysinit(void) {
    _setmode(_fileno(stdin), _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);
    signal(SIGINT, SIG_IGN);
}

VOID tmsgl(char *message) {
    if (!quiet)
        fprintf(stderr, "%s\n", message ? message : "");
}

char dopar(char value) {
    unsigned int parity;
    if (!xparity)
        return value;
    value &= 0177;
    switch (xparity) {
    case 'm':
        return value | 128;
    case 's':
        return value & 127;
    case 'o':
    case 'e':
        parity = (value & 15) ^ ((value >> 4) & 15);
        parity = (parity & 3) ^ ((parity >> 2) & 3);
        parity = (parity & 1) ^ ((parity >> 1) & 1);
        if (xparity == 'o')
            parity = 1 - parity;
        return value | (char)(parity << 7);
    default:
        return value;
    }
}

int ttopen(char *name) {
    (void)name;
    if (!nomodes) {
        if (!quiet)
            fprintf(stderr, "Windows G-Kermit supports external protocol mode only\n");
        return -1;
    }
    signal(SIGINT, doexit);
    streamok = -1;
    return 0;
}

int ttpkt(int parity) {
    xparity = parity;
    return nomodes ? 0 : -1;
}

int ttres(void) { return 0; }

int ttchk(void) {
    HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
    DWORD available = 0;
    if (!input || input == INVALID_HANDLE_VALUE ||
        !PeekNamedPipe(input, NULL, 0, NULL, &available, NULL))
        return -1;
    return available > INT_MAX ? INT_MAX : (int)available;
}

int ttflui(void) { return 0; }

int ttinl(char *dest, int max, int timeout, char eol, char soh, int turn) {
    int count = 0;
    int started = 0;
    int control_c = 0;
    int have_length = 0;
    int packet_length = 0;
    int long_packet_length = 0;
    char value = NUL;
    int result = 0;
    ULONGLONG deadline = timeout > 0 ? GetTickCount64() + (ULONGLONG)timeout * 1000ULL : 0;
    (void)eol;

    if (!dest || max < 1)
        return -2;
    dest[0] = NUL;
    for (;;) {
        result = read_byte(&value, deadline);
        if (result < 0)
            return result;
        if (xparity)
            value &= 0x7f;
        if (value == '\03') {
            if (++control_c > 2)
                doexit(1);
        } else {
            control_c = 0;
        }
        if (!started && value != soh)
            continue;
        started = 1;
        if (count >= max)
            return -2;
        dest[count++] = value;
        if (!have_length) {
            if (count == 2) {
                packet_length = xunchar(dest[1] & 0x7f);
                if (packet_length > 1)
                    have_length = 1;
            } else if (count == 5 && packet_length == 0) {
                long_packet_length = xunchar(dest[4] & 0x7f);
            } else if (count == 6 && packet_length == 0) {
                packet_length = long_packet_length * 95 + xunchar(dest[5] & 0x7f) + 5;
                have_length = 1;
            }
        }
        if (have_length && count > packet_length + 1) {
            if (turn && value != turn)
                continue;
            dest[count] = NUL;
            return count;
        }
    }
}

int ttol(char *data, int length) {
    HANDLE output = GetStdHandle(STD_OUTPUT_HANDLE);
    int offset = 0;
    if (!data || length < 0 || !output || output == INVALID_HANDLE_VALUE)
        return -1;
    if (xparity) {
        int index;
        for (index = 0; index < length; index++)
            data[index] = dopar(data[index]);
    }
    while (offset < length) {
        DWORD written = 0;
        DWORD remaining = (DWORD)(length - offset);
        if (!WriteFile(output, data + offset, remaining, &written, NULL) || written == 0)
            return -1;
        offset += (int)written;
    }
    return length;
}

long zchki(char *name) {
    struct _stat64 metadata;
    if (!name || path_stat(name, &metadata) < 0 || path_access(name, 4) < 0)
        return -1;
    if ((metadata.st_mode & _S_IFMT) != _S_IFREG)
        return -2;
    if (metadata.st_size > LONG_MAX) {
        errno = EFBIG;
        return -1;
    }
    return (long)metadata.st_size;
}

int zchko(char *name) {
    char *separator = NULL;
    int exists;
    if (!name || !*name)
        return -1;
    exists = (int)zchki(name);
    if (exists == -2)
        return -1;
    if (exists < 0) {
        char *cursor;
        strncpy_s(work, sizeof(work), name, _TRUNCATE);
        for (cursor = work; *cursor; cursor++) {
            if (*cursor == '/' || *cursor == '\\')
                separator = cursor;
        }
        if (separator)
            *separator = NUL;
        else
            strcpy_s(work, sizeof(work), ".");
        return path_access(work, 2) < 0 ? -1 : 0;
    }
    return path_access(name, 2) < 0 ? -1 : 0;
}

int zopeni(char *name) {
    long length = zchki(name);
    if (length < 0)
        return (int)length;
    ifp = path_open(name, L"rb");
    if (!ifp)
        return -1;
    zincnt = 0;
    zinptr = zinbuf;
    return 0;
}

int zopeno(char *name) {
    ofp = path_open(name, L"wb");
    if (!ofp)
        return -1;
    strncpy_s(ofile, sizeof(ofile), name, _TRUNCATE);
    return 0;
}

VOID zltor(char *local_name, char *packet_name, int max_length) {
    char *base = local_name;
    char *cursor;
    int output = 0;
    if (!local_name || !packet_name || max_length < 2)
        return;
    for (cursor = local_name; *cursor; cursor++) {
        if (*cursor == '/' || *cursor == '\\')
            base = cursor + 1;
    }
    for (cursor = base; *cursor && output < max_length - 1; cursor++) {
        unsigned char value = (unsigned char)*cursor;
        if (!literal && value < 128 && islower(value))
            value = (unsigned char)toupper(value);
        if (value < SP || value == '/' || value == '\\' || value == ':' || value == '*' ||
            value == '?' || value == '"' || value == '<' || value == '>' || value == '|')
            value = '_';
        packet_name[output++] = (char)value;
    }
    if (output == 0)
        packet_name[output++] = 'X';
    packet_name[output] = NUL;
}

int zbackup(char *name) {
    struct _stat64 metadata;
    char candidate[MAXPATHLEN + 16];
    int index;
    if (!name || !*name)
        return -1;
    if (path_stat(name, &metadata) < 0)
        return 0;
    for (index = 1; index <= 999; index++) {
        if (sprintf_s(candidate, sizeof(candidate), "%s.~%d~", name, index) < 0)
            return -1;
        if (path_stat(candidate, &metadata) < 0)
            return path_rename(name, candidate) == 0 ? 0 : -1;
    }
    return -1;
}

int zrtol(char *packet_name, char *local_name, int warn, int max_length) {
    char *base = packet_name;
    char *cursor;
    int output = 0;
    if (!packet_name || !local_name || max_length < 2)
        return -1;
    for (cursor = packet_name; *cursor; cursor++) {
        if (*cursor == '/' || *cursor == '\\')
            base = cursor + 1;
    }
    if (!strcmp(base, ".") || !strcmp(base, ".."))
        base = "NONAME";
    for (cursor = base; *cursor && output < max_length - 1; cursor++) {
        unsigned char value = (unsigned char)*cursor;
        if (value < SP || value == '/' || value == '\\' || value == ':' || value == '*' ||
            value == '?' || value == '"' || value == '<' || value == '>' || value == '|')
            value = '_';
        local_name[output++] = (char)value;
    }
    while (output > 0 && (local_name[output - 1] == '.' || local_name[output - 1] == ' '))
        local_name[--output] = NUL;
    if (output == 0) {
        strcpy_s(local_name, (size_t)max_length, "NONAME");
    } else {
        local_name[output] = NUL;
    }
    return warn ? zbackup(local_name) : 0;
}

int zclosi(void) {
    int result = ifp && fclose(ifp) == 0 ? 0 : -1;
    ifp = NULL;
    return result;
}

int zcloso(int cancelled) {
    int result = ofp && fclose(ofp) == 0 ? 0 : -1;
    ofp = NULL;
    if (cancelled && !keep)
        path_unlink(ofile);
    return result;
}

int zfillbuf(int text) {
    if (zincnt < 1) {
        if (text) {
            int value = 0;
            zincnt = 0;
            while (zincnt < MAXRECORD - 1 && (value = getc(ifp)) != EOF && value != '\n')
                zinbuf[zincnt++] = (char)value;
            if (value == '\n') {
                zinbuf[zincnt++] = '\r';
                zinbuf[zincnt++] = '\n';
            }
        } else {
            zincnt = (int)fread(zinbuf, sizeof(char), MAXRECORD, ifp);
        }
        zinbuf[zincnt] = NUL;
        if (zincnt == 0)
            return -1;
        zinptr = zinbuf;
    }
    zincnt--;
    return *zinptr++ & 0xff;
}
