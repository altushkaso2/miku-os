ENTRY(_libmiku_start)

SECTIONS {
    . = 0;

    .gnu.hash : { *(.gnu.hash) }
    .hash     : { *(.hash) }
    .dynsym   : { *(.dynsym) }
    .dynstr   : { *(.dynstr) }

    .text : {
        *(.text._libmiku_start)
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
