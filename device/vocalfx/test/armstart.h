/* Minimal ARM EABI entry for test programs linked against the DEVICE's glibc 2.4 with -nostartfiles: a modern crt1.o
 * calls __libc_start_main@GLIBC_2.34, which the device libc does not have. The dynamic linker has already initialised
 * libc by the time _start runs (shared libc is initialised through its DT_INIT), so the program can call libc directly. */
#ifndef ARMSTART_H
#define ARMSTART_H
int test_main(int argc, char **argv);
void _exit(int);
void test_entry(int argc, char **argv) __attribute__((used, noreturn));
void test_entry(int argc, char **argv) { _exit(test_main(argc, argv)); }
__asm__(".text\n.arm\n.global _start\n.type _start,%function\n_start:\n"
        "  mov fp, #0\n  ldr r0, [sp]\n  add r1, sp, #4\n  bic sp, sp, #7\n  bl test_entry\n");
#endif
