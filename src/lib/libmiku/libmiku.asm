bits 64
default rel

section .text

global miku_write
global miku_print
global miku_println
global miku_exit
global miku_itoa

miku_write:
    mov rax, 1
    syscall
    ret

miku_print:
    push rdi
    call _strlen
    mov rdx, rax
    pop rsi
    mov rdi, 1
    mov rax, 1
    syscall
    ret

miku_println:
    push rdi
    call _strlen
    mov rdx, rax
    pop rsi
    push rsi
    push rdx
    mov rdi, 1
    mov rax, 1
    syscall
    pop rdx
    pop rsi
    lea rsi, [rel _newline]
    mov rdx, 1
    mov rdi, 1
    mov rax, 1
    syscall
    ret

miku_exit:
    mov rax, 0
    syscall
    jmp $

miku_itoa:
    push rbx
    push r12
    push r13
    mov r12, rdi
    mov r13, rsi
    mov byte [r13], 0
    test r12, r12
    jns .positive
    mov byte [r13], '-'
    inc r13
    neg r12
.positive:
    mov rax, r12
    lea rbx, [r13 + 20]
    mov byte [rbx], 0
.digit_loop:
    dec rbx
    xor rdx, rdx
    mov rcx, 10
    div rcx
    add dl, '0'
    mov [rbx], dl
    test rax, rax
    jnz .digit_loop
.copy_loop:
    mov al, [rbx]
    mov [r13], al
    inc rbx
    inc r13
    test al, al
    jnz .copy_loop
    pop r13
    pop r12
    pop rbx
    ret

_strlen:
    xor rax, rax
    test rdi, rdi
    jz .done
.loop:
    cmp byte [rdi + rax], 0
    je .done
    inc rax
    jmp .loop
.done:
    ret

section .data
_newline: db 10
