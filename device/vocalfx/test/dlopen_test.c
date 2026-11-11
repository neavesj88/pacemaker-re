/* dlopen test for libvocalfx.so on the device's ld.so/glibc 2.4 (run under qemu-user): the library must load,
 * export its two introspection symbols, and REFUSE to patch because this executable is not pmplayer.real. */
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include "armstart.h"

int test_main(int argc, char **argv)
{
    const char *path = argc > 1 ? argv[1] : "/pmds/bin/libvocalfx.so";
    void *h = dlopen(path, RTLD_NOW);
    const char *(*ver)(void);
    int (*stat)(void);
    char line[300];
    if (!h) { snprintf(line, sizeof line, "FAIL dlopen(%s): %s\n", path, dlerror()); ssize_t w = write(1, line, strlen(line)); (void)w; return 1; }
    ver = (const char *(*)(void))dlsym(h, "vocalfx_version");
    stat = (int (*)(void))dlsym(h, "vocalfx_status");
    if (!ver || !stat) { ssize_t w = write(1, "FAIL missing symbols\n", 21); (void)w; return 1; }
    snprintf(line, sizeof line, "DLOPEN OK version=%s status=%d (expected -1 = not pmplayer.real, nothing patched)\n", ver(), stat());
    { ssize_t w = write(1, line, strlen(line)); (void)w; }
    return stat() == -1 ? 0 : 1;
}
