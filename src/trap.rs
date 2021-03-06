use crate::cpu::TrapFrame;
use crate::{plic, println};

#[no_mangle]
extern "C" fn m_trap(
    epc: usize,
    tval: usize,
    cause: usize,
    hart: usize,
    _status: usize,
    _frame: &mut TrapFrame,
) -> usize {
    // We're going to handle all traps in machine mode.
    // RISC-V lets us delegate to supervisor mode,
    // but switching out SATP (virtual memory) gets hairy.
    let is_async = {
        if cause >> 63 & 1 == 1 {
            true
        } else {
            false
        }
    };
    // The cause contains the type of trap (sync, async) as well as the cause
    // number. So, here we narrow down just the cause number.
    let cause_num = cause & 0xfff;
    let mut return_pc = epc;
    if is_async {
        // Asynchronous trap
        match cause_num {
            3 => {
                // Machine software
                println!("Machine software interrupt CPU#{}", hart);
            }
            7 => unsafe {
                // Machine timer
                let mtimecmp = 0x0200_4000 as *mut u64;
                let mtime = 0x0200_bff8 as *const u64;
                // The frequency given by QEMU is 10_000_000 Hz, so this sets
                // the next interrupt to fire one second from now.
                mtimecmp.write_volatile(mtime.read_volatile() + 10_000_000);
                println!("Timer async trap, CPU#{} -> {}", hart, cause_num);
            },
            11 => {
                // Machine external (interrupt from Platform Interrupt Controller (PLIC))
                // println!("Machine external interrupt CPU#{}", hart);
                // We will check the next interrupt. If the interrupt isn't available, this will
                // give us None. However, that would mean we got a spurious interrupt, unless we
                // get an interrupt from a non-PLIC source. This is the main reason that the PLIC
                // hardwires the id 0 to 0, so that we can use it as an error case.
                if let Some(interrupt) = plic::next() {
                    // If we get here, we've got an interrupt from the claim register. The PLIC will
                    // automatically prioritize the next interrupt, so when we get it from claim, it
                    // will be the next in priority order.
                    match interrupt {
                        10 => {
                            // Interrupt 10 is the UART interrupt.
                            // We would typically set this to be handled out of the interrupt context,
                            // but we're testing here! C'mon!
                            // We haven't yet used the singleton pattern for my_uart, but remember, this
                            // just simply wraps 0x1000_0000 (UART).
                            let mut my_uart = crate::uart::Uart::new(0x1000_0000);
                            // If we get here, the UART better have something! If not, what happened??
                            if let Some(c) = my_uart.get() {
                                match c {
                                    8 => {
                                        // This is a backspace, so we
                                        // essentially have to write a space and
                                        // backup again:
                                        println!("{} {}", 8 as char, 8 as char);
                                    }
                                    10 | 13 => {
                                        // Newline or carriage-return
                                        println!();
                                    }
                                    _ => {
                                        crate::print!("Got => {}\n", c as char);
                                    }
                                }
                            }
                        }
                        _ => {
                            println!("Non-UART external interrupt: {}", interrupt);
                        }
                    }
                    plic::complete(interrupt);
                }
                // Machine external (interrupt from Platform Interrupt Controller (PLIC))
                println!("Machine external interrupt CPU#{}", hart);
            }
            _ => {
                panic!("Unhandled async trap CPU#{} -> {}\n", hart, cause_num);
            }
        }
    } else {
        // Synchronous trap
        match cause_num {
            2 => {
                // Illegal instruction
                panic!(
                    "Illegal instruction CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
            }
            8 => {
                // Environment (system) call from User mode
                println!("E-call from User mode! CPU#{} -> 0x{:08x}", hart, epc);
                return_pc += 4;
            }
            9 => {
                // Environment (system) call from Supervisor mode
                println!("E-call from Supervisor mode! CPU#{} -> 0x{:08x}", hart, epc);
                return_pc += 4;
            }
            11 => {
                // Environment (system) call from Machine mode
                panic!("E-call from Machine mode! CPU#{} -> 0x{:08x}\n", hart, epc);
            }
            // Page faults
            12 => {
                // Instruction page fault
                println!(
                    "Instruction page fault CPU#{} -> 0x{:08x}: 0x{:08x}",
                    hart, epc, tval
                );
                return_pc += 4;
            }
            13 => {
                // Load page fault
                println!(
                    "Load page fault CPU#{} -> 0x{:08x}: 0x{:08x}",
                    hart, epc, tval
                );
                return_pc += 4;
            }
            15 => {
                // Store page fault
                println!(
                    "Store page fault CPU#{} -> 0x{:08x}: 0x{:08x}",
                    hart, epc, tval
                );
                return_pc += 4;
            }
            _ => {
                panic!(
                    "Unhandled sync trap CPU#{} -> {} at {}\n",
                    hart, cause_num, tval
                );
            }
        }
    }

    // Finally, return the updated program counter
    return_pc
}
