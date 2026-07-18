/* GPL-2.0-or-later compatibility declarations injected into G-Kermit sources. */
#ifndef CNSHELL_GKERMIT_WINDOWS_COMPAT_H
#define CNSHELL_GKERMIT_WINDOWS_COMPAT_H

unsigned int gkermit_windows_sleep(unsigned int seconds);
#define sleep gkermit_windows_sleep

#endif

