# Assembly-level trap handler.

    .option norvc
    .equ NUM_GP_REGS, 32
    .equ NUM_FP_REGS, 32
    .equ REG_SIZE, 8
    .equ MAX_CPUS, 8

# Use macros for saving and restoring multiple registers
.macro  load_gp base=t6
    ld ra,  (0 * REG_SIZE)(\base)
	ld sp,  (1 * REG_SIZE)(\base)
	ld gp,  (2 * REG_SIZE)(\base)
	ld tp,  (3 * REG_SIZE)(\base)
	ld t0,  (4 * REG_SIZE)(\base)
	ld t1,  (5 * REG_SIZE)(\base)
	ld t2,  (6 * REG_SIZE)(\base)
	ld s0,  (7 * REG_SIZE)(\base)
	ld s1,  (8 * REG_SIZE)(\base)
	ld a0,  (9 * REG_SIZE)(\base)
	ld a1,  (10 * REG_SIZE)(\base)
	ld a2,  (11 * REG_SIZE)(\base)
	ld a3,  (12 * REG_SIZE)(\base)
	ld a4,  (13 * REG_SIZE)(\base)
	ld a5,  (14 * REG_SIZE)(\base)
	ld a6,  (15 * REG_SIZE)(\base)
	ld a7,  (16 * REG_SIZE)(\base)
	ld s2,  (17 * REG_SIZE)(\base)
	ld s3,  (18 * REG_SIZE)(\base)
	ld s4,  (19 * REG_SIZE)(\base)
	ld s5,  (20 * REG_SIZE)(\base)
	ld s6,  (21 * REG_SIZE)(\base)
	ld s7,  (22 * REG_SIZE)(\base)
	ld s8,  (23 * REG_SIZE)(\base)
	ld s9,  (24 * REG_SIZE)(\base)
	ld s10, (25 * REG_SIZE)(\base)
	ld s11, (26 * REG_SIZE)(\base)
	ld t3, (27 * REG_SIZE)(\base)
	ld t4, (28 * REG_SIZE)(\base)
	ld t5, (29 * REG_SIZE)(\base)
	ld t6, (30 * REG_SIZE)(\base)
.endm

.macro save_gp base=t6
    sd ra,  (0 * REG_SIZE)(\base)
	sd sp,  (1 * REG_SIZE)(\base)
	sd gp,  (2 * REG_SIZE)(\base)
	sd tp,  (3 * REG_SIZE)(\base)
	sd t0,  (4 * REG_SIZE)(\base)
	sd t1,  (5 * REG_SIZE)(\base)
	sd t2,  (6 * REG_SIZE)(\base)
	sd s0,  (7 * REG_SIZE)(\base)
	sd s1,  (8 * REG_SIZE)(\base)
	sd a0,  (9 * REG_SIZE)(\base)
	sd a1,  (10 * REG_SIZE)(\base)
	sd a2,  (11 * REG_SIZE)(\base)
	sd a3,  (12 * REG_SIZE)(\base)
	sd a4,  (13 * REG_SIZE)(\base)
	sd a5,  (14 * REG_SIZE)(\base)
	sd a6,  (15 * REG_SIZE)(\base)
	sd a7,  (16 * REG_SIZE)(\base)
	sd s2,  (17 * REG_SIZE)(\base)
	sd s3,  (18 * REG_SIZE)(\base)
	sd s4,  (19 * REG_SIZE)(\base)
	sd s5,  (20 * REG_SIZE)(\base)
	sd s6,  (21 * REG_SIZE)(\base)
	sd s7,  (22 * REG_SIZE)(\base)
	sd s8,  (23 * REG_SIZE)(\base)
	sd s9,  (24 * REG_SIZE)(\base)
	sd s10, (25 * REG_SIZE)(\base)
	sd s11, (26 * REG_SIZE)(\base)
	sd t3, (27 * REG_SIZE)(\base)
	sd t4, (28 * REG_SIZE)(\base)
	sd t5, (29 * REG_SIZE)(\base)
	sd t6, (30 * REG_SIZE)(\base)
.endm

# .macro save_fp, i, base_reg=t6
#     fsd f\i, ((NUM_FP_REGS + (\i)) * REG_SIZE)(\base_reg)
# .endm

# .macro load_gp i, base_reg=t6
#     fld f\i, ((NUM_FP_REGS + (\i)) * REG_SIZE)(\base_reg)
# .endm

    .section .text
    .global m_trap_vector
# This must be aligned by 4 since the last two bits
# of the mtvec register do not contribute to the address of this vector.
.align 4
m_trap_vector:
    # All registers are volatile here, we need to save them before we do anything.
    csrrw t6, mscratch, t6
    # csrrw will atomically swap t6 into mscratch and the old value of mscratch into t6.
    # This is nice because we just switched values and didn't destroy anything -- all atomically!
    # in cpu.rs we have a structure of:
    # 32 gp regs		0
    # 32 fp regs		256
    # SATP register	512
    # Trap stack       520
    # CPU HARTID		528
    # We use t6 as the temporary register because it is the very bottom register (x31)
    save_gp t6

    # Save the actual t6 register, which we swapped into mscratch
    mv      t5,     t6
    csrr    t6,     mscratch
    sd      t6,     (30 * REG_SIZE)(t5)

    # Restore the kernel trap frame into mscratch
    csrw    mscratch,t5

    # Get ready to go into Rust (trap.rs)
    # We don't want to write into the user's statck or whomever messed with us here
    csrr    a0,     mepc
    csrr    a1,     mtval
    csrr    a2,     mcause
    csrr    a3,     mhartid
    csrr    a4,     mstatus
    mv      a5,     t5
    ld      sp,     520(a5)
    call    m_trap

    # When we get here, we've returned from m_trap, restore registers and return
    # m_trap will return the return address via a0
    csrw    mepc,   a0

    # Restore all GP registers
    csrr t6, mscratch
    load_gp t6
    # Since we ran this loop 31 times starting with i = 1,
    # the last one loaded t6 back to its original value.

    mret

.global make_syscall
make_syscall:
    ecall
    ret