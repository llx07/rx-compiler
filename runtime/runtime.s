	.attribute	4, 16
	.attribute	5, "rv32i2p1_m2p0_zmmul1p0"
	.file	"runtime.c"
	.text
	.globl	__rx_alloc                      # -- Begin function __rx_alloc
	.p2align	2
	.type	__rx_alloc,@function
__rx_alloc:                             # @__rx_alloc
# %bb.0:
	tail	malloc
.Lfunc_end0:
	.size	__rx_alloc, .Lfunc_end0-__rx_alloc
                                        # -- End function
	.globl	get_i32                         # -- Begin function get_i32
	.p2align	2
	.type	get_i32,@function
get_i32:                                # @get_i32
# %bb.0:
	addi	sp, sp, -16
	sw	ra, 12(sp)                      # 4-byte Folded Spill
	lui	a0, %hi(.L.str)
	addi	a0, a0, %lo(.L.str)
	addi	a1, sp, 8
	call	scanf
	lw	a0, 8(sp)
	lw	ra, 12(sp)                      # 4-byte Folded Reload
	addi	sp, sp, 16
	ret
.Lfunc_end1:
	.size	get_i32, .Lfunc_end1-get_i32
                                        # -- End function
	.globl	print_i32                       # -- Begin function print_i32
	.p2align	2
	.type	print_i32,@function
print_i32:                              # @print_i32
# %bb.0:
	lui	a1, %hi(.L.str)
	addi	a1, a1, %lo(.L.str)
	mv	a2, a0
	mv	a0, a1
	mv	a1, a2
	tail	printf
.Lfunc_end2:
	.size	print_i32, .Lfunc_end2-print_i32
                                        # -- End function
	.globl	println_i32                     # -- Begin function println_i32
	.p2align	2
	.type	println_i32,@function
println_i32:                            # @println_i32
# %bb.0:
	lui	a1, %hi(.L.str.1)
	addi	a1, a1, %lo(.L.str.1)
	mv	a2, a0
	mv	a0, a1
	mv	a1, a2
	tail	printf
.Lfunc_end3:
	.size	println_i32, .Lfunc_end3-println_i32
                                        # -- End function
	.globl	__rx_vec_new                    # -- Begin function __rx_vec_new
	.p2align	2
	.type	__rx_vec_new,@function
__rx_vec_new:                           # @__rx_vec_new
# %bb.0:
	sw	zero, 0(a0)
	sw	zero, 4(a0)
	sw	zero, 8(a0)
	ret
.Lfunc_end4:
	.size	__rx_vec_new, .Lfunc_end4-__rx_vec_new
                                        # -- End function
	.globl	__rx_vec_new_with_size          # -- Begin function __rx_vec_new_with_size
	.p2align	2
	.type	__rx_vec_new_with_size,@function
__rx_vec_new_with_size:                 # @__rx_vec_new_with_size
# %bb.0:
	addi	sp, sp, -16
	sw	ra, 12(sp)                      # 4-byte Folded Spill
	sw	s0, 8(sp)                       # 4-byte Folded Spill
	sw	s1, 4(sp)                       # 4-byte Folded Spill
	mv	s0, a1
	mv	s1, a0
	beqz	a1, .LBB5_2
# %bb.1:
	mul	a0, a2, s0
	call	malloc
	j	.LBB5_3
.LBB5_2:
	li	a0, 0
.LBB5_3:
	sw	a0, 0(s1)
	sw	s0, 4(s1)
	sw	s0, 8(s1)
	lw	ra, 12(sp)                      # 4-byte Folded Reload
	lw	s0, 8(sp)                       # 4-byte Folded Reload
	lw	s1, 4(sp)                       # 4-byte Folded Reload
	addi	sp, sp, 16
	ret
.Lfunc_end5:
	.size	__rx_vec_new_with_size, .Lfunc_end5-__rx_vec_new_with_size
                                        # -- End function
	.globl	__rx_vec_len                    # -- Begin function __rx_vec_len
	.p2align	2
	.type	__rx_vec_len,@function
__rx_vec_len:                           # @__rx_vec_len
# %bb.0:
	lw	a0, 4(a0)
	ret
.Lfunc_end6:
	.size	__rx_vec_len, .Lfunc_end6-__rx_vec_len
                                        # -- End function
	.globl	__rx_vec_is_empty               # -- Begin function __rx_vec_is_empty
	.p2align	2
	.type	__rx_vec_is_empty,@function
__rx_vec_is_empty:                      # @__rx_vec_is_empty
# %bb.0:
	lw	a0, 4(a0)
	seqz	a0, a0
	ret
.Lfunc_end7:
	.size	__rx_vec_is_empty, .Lfunc_end7-__rx_vec_is_empty
                                        # -- End function
	.globl	__rx_vec_push                   # -- Begin function __rx_vec_push
	.p2align	2
	.type	__rx_vec_push,@function
__rx_vec_push:                          # @__rx_vec_push
# %bb.0:
	addi	sp, sp, -32
	sw	ra, 28(sp)                      # 4-byte Folded Spill
	sw	s0, 24(sp)                      # 4-byte Folded Spill
	sw	s1, 20(sp)                      # 4-byte Folded Spill
	sw	s2, 16(sp)                      # 4-byte Folded Spill
	sw	s3, 12(sp)                      # 4-byte Folded Spill
	sw	s4, 8(sp)                       # 4-byte Folded Spill
	mv	s1, a2
	mv	s0, a0
	lw	a0, 4(a0)
	lw	a2, 8(s0)
	bne	a0, a2, .LBB8_3
# %bb.1:
	mv	s4, a1
	bnez	a0, .LBB8_4
# %bb.2:
	li	s3, 4
	j	.LBB8_5
.LBB8_3:
	lw	s2, 0(s0)
	j	.LBB8_9
.LBB8_4:
	slli	s3, a0, 1
.LBB8_5:
	mul	a0, s3, s1
	call	malloc
	lw	a2, 4(s0)
	mv	s2, a0
	beqz	a2, .LBB8_7
# %bb.6:
	lw	a1, 0(s0)
	mul	a2, a2, s1
	mv	a0, s2
	call	memcpy
	lw	a0, 4(s0)
	j	.LBB8_8
.LBB8_7:
	li	a0, 0
.LBB8_8:
	mv	a1, s4
	sw	s2, 0(s0)
	sw	s3, 8(s0)
.LBB8_9:
	mul	a0, a0, s1
	add	a0, s2, a0
	mv	a2, s1
	call	memcpy
	lw	a0, 4(s0)
	addi	a0, a0, 1
	sw	a0, 4(s0)
	lw	ra, 28(sp)                      # 4-byte Folded Reload
	lw	s0, 24(sp)                      # 4-byte Folded Reload
	lw	s1, 20(sp)                      # 4-byte Folded Reload
	lw	s2, 16(sp)                      # 4-byte Folded Reload
	lw	s3, 12(sp)                      # 4-byte Folded Reload
	lw	s4, 8(sp)                       # 4-byte Folded Reload
	addi	sp, sp, 32
	ret
.Lfunc_end8:
	.size	__rx_vec_push, .Lfunc_end8-__rx_vec_push
                                        # -- End function
	.globl	__rx_vec_remove                 # -- Begin function __rx_vec_remove
	.p2align	2
	.type	__rx_vec_remove,@function
__rx_vec_remove:                        # @__rx_vec_remove
# %bb.0:
	addi	sp, sp, -32
	sw	ra, 28(sp)                      # 4-byte Folded Spill
	sw	s0, 24(sp)                      # 4-byte Folded Spill
	sw	s1, 20(sp)                      # 4-byte Folded Spill
	sw	s2, 16(sp)                      # 4-byte Folded Spill
	sw	s3, 12(sp)                      # 4-byte Folded Spill
	mv	s0, a3
	mv	s1, a0
	lw	s3, 0(a0)
	mv	s2, a1
	mul	a0, a3, a1
	add	s3, s3, a0
	mv	a0, a2
	mv	a1, s3
	mv	a2, a3
	call	memcpy
	lw	a0, 4(s1)
	add	a1, s3, s0
	not	a2, s2
	add	a0, a0, a2
	mul	a2, a0, s0
	mv	a0, s3
	call	memmove
	lw	a0, 4(s1)
	addi	a0, a0, -1
	sw	a0, 4(s1)
	lw	ra, 28(sp)                      # 4-byte Folded Reload
	lw	s0, 24(sp)                      # 4-byte Folded Reload
	lw	s1, 20(sp)                      # 4-byte Folded Reload
	lw	s2, 16(sp)                      # 4-byte Folded Reload
	lw	s3, 12(sp)                      # 4-byte Folded Reload
	addi	sp, sp, 32
	ret
.Lfunc_end9:
	.size	__rx_vec_remove, .Lfunc_end9-__rx_vec_remove
                                        # -- End function
	.type	.L.str,@object                  # @.str
	.section	.rodata.str1.1,"aMS",@progbits,1
.L.str:
	.asciz	"%d"
	.size	.L.str, 3

	.type	.L.str.1,@object                # @.str.1
.L.str.1:
	.asciz	"%d\n"
	.size	.L.str.1, 4

	.ident	"Homebrew clang version 22.1.8"
	.section	".note.GNU-stack","",@progbits
	.addrsig
