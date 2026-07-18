/* GPL-2.0-or-later compatibility declarations injected into G-Kermit sources. */
#ifndef CNSHELL_GKERMIT_WINDOWS_COMPAT_H
#define CNSHELL_GKERMIT_WINDOWS_COMPAT_H

#ifndef __STDC__
#define __STDC__ 1
#endif

unsigned int gkermit_windows_sleep(unsigned int seconds);
#define sleep gkermit_windows_sleep

/*
 * G-Kermit 2.01 leaves one debug-only gptr reference outside its
 * NOGETENV guard. CNshell intentionally disables environment parsing, so
 * provide the value that the guarded variable would otherwise start with.
 */
#ifdef NOGETENV
#define gptr ((char *)0)
#endif

#endif
