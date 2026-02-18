OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

PHDRS
{
    requests PT_LOAD FLAGS((1 << 2));              /* r   */
    text     PT_LOAD FLAGS((1 << 0) | (1 << 2));   /* rx  */
    rodata   PT_LOAD FLAGS((1 << 2));               /* r   */
    data     PT_LOAD FLAGS((1 << 1) | (1 << 2));    /* rw  */
}

SECTIONS
{
    . = 0xFFFFFFFF80000000;

    .requests_start_marker : ALIGN(0x1000) {
        KEEP(*(.requests_start_marker))
    } :requests

    .requests : {
        KEEP(*(.requests))
    } :requests

    .requests_end_marker : {
        KEEP(*(.requests_end_marker))
    } :requests

    .text : ALIGN(0x1000) {
        *(.text .text.*)
    } :text

    .rodata : ALIGN(0x1000) {
        *(.rodata .rodata.*)
    } :rodata

    .data : ALIGN(0x1000) {
        *(.data .data.*)
        *(.got .got.*)
    } :data

    .bss : ALIGN(0x1000) {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ : {
        *(.eh_frame*)
        *(.note*)
        *(.comment*)
    }
}
