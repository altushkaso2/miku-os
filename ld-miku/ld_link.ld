ENTRY(_ldstart)

SECTIONS {
    . = 0x7F0000000000;

    .text : {
        *(.text._ldstart)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    . = ALIGN(4096);

    .data.rel.ro : { *(.data.rel.ro .data.rel.ro.*) }
    .dynamic     : { *(.dynamic) }

    . = ALIGN(4096);

    .data : { *(.data .data.*) }

    . = ALIGN(4096);

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    }

    .rela.dyn : { *(.rela.dyn) }
    .rela.plt : { *(.rela.plt) }

    /DISCARD/ : {
        *(.note*) *(.comment*) *(.eh_frame*) *(.gnu.version*)
    }
}
