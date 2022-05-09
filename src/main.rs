#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

pub mod cpu;
pub mod kmem;
pub mod page;
pub mod trap;
pub mod uart;
pub mod plic;

use core::{
    arch::{asm, global_asm},
    mem::size_of,
};

#[macro_use]
extern crate alloc;

use alloc::{boxed::Box, string::String};

global_asm!(include_str!("asm/boot.S"));
global_asm!(include_str!("asm/trap.S"));
global_asm!(include_str!("asm/mem.S"));

// The following symbols come from asm/mem.S. We can use
// the symbols directly, but the address of the symbols
// themselves are their values, which can cause issues.
// Instead, I created doubleword values in mem.S in the .rodata and .data
// sections.
extern "C" {
    static TEXT_START: usize;
    static TEXT_END: usize;
    static DATA_START: usize;
    static DATA_END: usize;
    static RODATA_START: usize;
    static RODATA_END: usize;
    static BSS_START: usize;
    static BSS_END: usize;
    static KERNEL_STACK_START: usize;
    static KERNEL_STACK_END: usize;
    static HEAP_START: usize;
    static HEAP_SIZE: usize;
}

/// RUST MACROS
#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        let mut uart = crate::uart::Uart::new(0x1000_0000);
        uart.print(format_args!($fmt $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! println
{
	() => ({
        let mut uart = crate::uart::Uart::new(0x1000_0000);
		uart.print(format_args!(concat!("\r\n")));
	});
	($fmt: literal $(, $($arg: tt)+)?) => ({
        let mut uart = crate::uart::Uart::new(0x1000_0000);
        uart.print(format_args!(concat!($fmt, "\n") $(, $($arg)+)?));
	});
}

// / LANGUAGE STRUCTURES / FUNCTIONS
#[no_mangle]
extern "C" fn eh_personality() {}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    print!("Aborting: ");
    if let Some(_p) = info.location() {
        println!(
            "line:{}, file:{}@{}",
            _p.line(),
            _p.file(),
            info.message().unwrap()
        );
    } else {
        println!("no information available.");
    }
    abort();
}

#[no_mangle]
extern "C" fn abort() -> ! {
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

/// Identity map range (virtual address = physical address)
/// Takes a contiguous allocation of memory and maps it using PAGE_SIZE
/// This assumes that start <= end
/// MMU操作基本单位是页，将start和end进行页对齐
/// 设置bits，就是设置映射页的属性
pub fn id_map_range(root: &mut page::Table, start: usize, end: usize, bits: i64) {
    let mut mem_addr = start & !(page::PAGE_SIZE - 1);
    let num_kb_pages = (page::align_val(end, 12) - mem_addr) / page::PAGE_SIZE;
    // I named this num_kb_pages for future expansion when
    // I decide to allow for GiB (2^30) and 2MiB (2^21) page
    // sizes. However, the overlapping memory regions are causing
    // nightmares.

    // map函数将会将这个范围进行映射（就是给虚拟地址映射到物理地址）
    // 每次是一页操作
    for _ in 0..num_kb_pages {
        page::map(root, mem_addr, mem_addr, bits, 0);
        mem_addr += 1 << 12;
    }
}

// ///////////////////////////////////
// / ENTRY POINT
// ///////////////////////////////////
#[no_mangle]
extern "C" fn kinit() {
    // We created kinit, which runs in super-duper mode 3 called "machine mode".
    // The job of kinit() is to get us into supervisor mode as soon as possible.
    // Interrupts are disabled for the duration of kinit()
    uart::Uart::new(0x1000_0000).init();
    page::init();
    kmem::init();

    // Map heap allocation
    let root_ptr = kmem::get_page_table();
    let root_u = root_ptr as usize;
    let mut root = unsafe { root_ptr.as_mut().unwrap() };
    let kheap_head = kmem::get_head() as usize;
    let total_pages = kmem::get_num_allocations();
    println!();
    println!();
    unsafe {
        println!("TEXT:   0x{:x} -> 0x{:x}", TEXT_START, TEXT_END);
        println!("RODATA: 0x{:x} -> 0x{:x}", RODATA_START, RODATA_END);
        println!("DATA:   0x{:x} -> 0x{:x}", DATA_START, DATA_END);
        println!("BSS:    0x{:x} -> 0x{:x}", BSS_START, BSS_END);
        println!("STACK:  0x{:x} -> 0x{:x}", KERNEL_STACK_START, KERNEL_STACK_END);
        println!("HEAP:   0x{:x} -> 0x{:x}", kheap_head, kheap_head + total_pages * page::PAGE_SIZE);
    }
    id_map_range(
        &mut root,
        kheap_head,
        kheap_head + total_pages * page::PAGE_SIZE,
        page::EntryBits::ReadWrite.val(),
    );
    unsafe {
        // Map heap descriptors
        let num_pages = HEAP_SIZE / page::PAGE_SIZE;
        id_map_range(
            &mut root,
            HEAP_START,
            HEAP_START + num_pages,
            page::EntryBits::ReadWrite.val(),
        );

        // Map executable section
        id_map_range(
            &mut root,
            TEXT_START,
            TEXT_END,
            page::EntryBits::ReadExecute.val(),
        );

        // Map rodata section
        // We put the ROdata section into the text section, so they can
        // potentially overlap however, we only care that it's read
        // only.
        id_map_range(
            &mut root,
            RODATA_START,
            RODATA_END,
            page::EntryBits::ReadExecute.val(),
        );

        // Map data section
        id_map_range(
            &mut root,
            DATA_START,
            DATA_END,
            page::EntryBits::ReadWrite.val(),
        );

        // Map bss section
        id_map_range(
            &mut root,
            BSS_START,
            BSS_END,
            page::EntryBits::ReadWrite.val(),
        );

        // Map kernel stack
        id_map_range(
            &mut root,
            KERNEL_STACK_START,
            KERNEL_STACK_END,
            page::EntryBits::ReadWrite.val(),
        );
    }
    // UART
    page::map(
        &mut root,
        0x1000_0000,
        0x1000_0100,
        page::EntryBits::ReadWrite.val(),
        0,
    );
    // CLINT
    //  -> MSIP
    id_map_range(
        &mut root,
        0x0200_0000,
        0x0200_ffff,
        page::EntryBits::ReadWrite.val(),
    );
    // PLIC
    id_map_range(
        &mut root,
        0x0c00_0000,
        0x0c00_2001,
        page::EntryBits::ReadWrite.val(),
    );

    id_map_range(
        &mut root,
        0x0c20_0000,
        0x0c20_8001,
        page::EntryBits::ReadWrite.val(),
    );

    // When we return from here, we'll go back to boot.S and switch into supervisor mode
    // We will return the SATP register to be written when we return.
    // root_u is the root page table's address. When stored into the SATP register,
    // this is divided by 4 KiB (right shift by 12 bits).
    // We enable the MMU by setting mode 8. Bits 63, 62, 61, 60 determine the mode.
    // 0 = Bare (no translation)
    // 8 = Sv39
    // 9 = Sv48
    // build_satp has these parameters: mode, asid, page table address.
    let satp_value = cpu::build_satp(cpu::SatpMode::Sv39, 0, root_u);
    unsafe {
        // We have to store the kernel's table.The tables will be moved back
        // and forth between the kernel's table and user applications' tables.
        // Note that we're writing the physical address of the trap frame.
        cpu::mscratch_write((&mut cpu::KERNEL_TRAP_FRAME[0] as *mut cpu::TrapFrame) as usize);
        cpu::sscratch_write(cpu::mscratch_read());
        cpu::KERNEL_TRAP_FRAME[0].satp = satp_value;

        // Move the stack pointer to the very bottom. The stack is actually in a non-mapped page.
        // The stack is decrement-before push and increment after pop.
        // Therefore, the stack will be allocated (decremented) before it is stored.
        cpu::KERNEL_TRAP_FRAME[0].trap_stack = page::zalloc(1).add(page::PAGE_SIZE);
        id_map_range(
            &mut root,
            cpu::KERNEL_TRAP_FRAME[0].trap_stack.sub(page::PAGE_SIZE) as usize,
            cpu::KERNEL_TRAP_FRAME[0].trap_stack as usize,
            page::EntryBits::ReadWrite.val(),
        );

        // The trap frame itself is stored in the mscratch register.
        id_map_range(
            &mut root,
            cpu::mscratch_read(),
            cpu::mscratch_read() + size_of::<cpu::TrapFrame>(),
            page::EntryBits::ReadWrite.val(),
        );
        // page::print_page_allocations();
        // The following shows how we're going to walk to translate a virtual
        // address into a physical address. We will use this whenever a user
        // space application requires services. Since the user space application
        // only knows virtual addresses, we have to translate silently behind
        // the scenes.
        let p = 0x8009_3000 as usize;
        let m = page::virt_to_phys(&root, p).unwrap_or(0);
        println!("Walk 0x{:x} = 0x{:x}", p, m);
    }
    // The following shows how we're going to walk to translate a virtual
    // address into a physical address. We will use this whenever a user
    // space application requires services. Since the user space application
    // only knows virtual addresses, we have to translate silently behind the scenes.
    println!("Setting 0x{:x}", satp_value);
    println!("Scratch ret = 0x{:x}", cpu::mscratch_read());
    cpu::satp_write(satp_value);
    cpu::satp_fence_asid(0);
}

#[no_mangle]
extern "C" fn kinit_hart(hartid: usize) {
    // All non-0 harts initialize here
    unsafe {
        // We have to store the kernel's table. The tables will be moved
        // back and forth between the kernel's table and user application's table
        cpu::mscratch_write((&mut cpu::KERNEL_TRAP_FRAME[hartid] as *mut cpu::TrapFrame) as usize);
        // Copy the same mscratch over to the supervisor version of the same register.
        cpu::sscratch_write(cpu::mscratch_read());
        cpu::KERNEL_TRAP_FRAME[hartid].hartid = hartid;
        // We can't do the following until zalloc() is locked, but we
        // don't have locks, yet :(
        // cpu::KERNEL_TRAP_FRAME[hartid].satp = cpu::KERNEL_TRAP_FRAME[0].satp;
        // cpu::KERNEL_TRAP_FRAME[hartid].trap_stack = page::zalloc(1);
    }
}

#[no_mangle]
extern "C" fn kmain() {
    // Main should initialize all sub-systems and get
    // ready to start scheduling. The last thing this
    // should do is start the timer.

    // Let's try using our newly minted UART by initializing it first.
    // The UART is sitting at MMIO address 0x1000_0000, so for testing
    // now, lets connect to it and see if we can initialize it and write
    // to it.
    // let mut my_uart = uart::Uart::new(0x1000_0000);

    // Create a new scope so that we can test the global allocator and deallocator
    {
        // We have the global allocator, so let's see if that works!
        let k = Box::<u32>::new(100);
        println!("Boxed value = {}", *k);
        // The following comes from the Rust documentation: some bytes, in a vector
        let sparkle_heart = vec![240, 159, 146, 150];
        // We know these bytes are valid, so we'll use `unwrap()`.
        let sparkle_heart = String::from_utf8(sparkle_heart).unwrap();
        println!("String = {}", sparkle_heart);
        println!("\n\nAllocations of a box, vector, and string");
        kmem::print_table();
    }
    // Now test println! macro!
    println!("\n\nEverything should now be free:");
    kmem::print_table();

    unsafe {
        // Set the next machine timer to fire.
        let mtimecmp = 0x0200_4000 as *mut u64;
        let mtime = 0x0200_bff8 as *const u64;
        // The frequency given by QEMU is 10_000_000 Hz, so this sets
        // the next interrupt to fire one second from now.
        mtimecmp.write_volatile(mtime.read_volatile() + 10_000_000);

        // Let's cause a page fault and see what happens. This should trap
        // to m_trap under trap.rs
        // let v = 0x0 as *mut u64;
        // v.write_volatile(0);
        // v.read_volatile();
    }
    // Now see if we can read stuff:
    // Usually we can use #[test] modules in Rust, but it would convolute the
    // task at hand. So, we'll just add testing snippets.
    // Let's set up the interrupt system via the PLIC. We have to set the threshold to something
	// that won't mask all interrupts.
	println!("Setting up interrupts and PLIC...");
	// We lower the threshold wall so our interrupts can jump over it.
	plic::set_threshold(0);
	// VIRTIO = [1..8]
	// UART0 = 10
	// PCIE = [32..35]
	// Enable the UART interrupt.
	plic::enable(10);
	plic::set_priority(10, 1);
	println!("UART interrupts have been enabled and are awaiting your command");
    loop {}
}
