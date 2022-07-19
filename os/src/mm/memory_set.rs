use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use core::result::Result;
use lazy_static::*;
use riscv::register::satp;

use super::address::PhysPageNum;
use super::frame_allocator::frame_alloc;
use super::{
    address::{PhysAddr, StepByOne, VPNRange, VirtAddr, VirtPageNum},
    frame_allocator::FrameTracker,
    page_table::{PTEFlags, PageTable, PageTableEntry},
};
use crate::config::{MEMORY_END, MMIO, PAGE_SIZE, TRAMPOLINE};

extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MapType {
    Identical,
    Framed,
}

bitflags! {
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}

// a continued segment of virtual address
pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        Self {
            vpn_range: VPNRange::new(start_va.floor(), end_va.ceil()),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }

    pub fn from_another(another: &Self) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
        }
    }

    pub fn map_one(
        &mut self,
        page_table: &mut PageTable,
        vpn: VirtPageNum,
    ) -> Result<(), &'static str> {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Framed => {
                let frame = frame_alloc();
                // handle
                match frame {
                    Some(frame) => {
                        ppn = frame.ppn;
                        self.data_frames.insert(vpn, frame);
                    }
                    None => {
                        return Err("No enough physical space");
                    }
                }
            }
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
        }
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        page_table.map(vpn, ppn, pte_flags);
        Ok(())
    }

    #[allow(unused)]
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }

    pub fn map(&mut self, page_table: &mut PageTable) -> Result<(), &'static str> {
        for vpn in self.vpn_range {
            match self.map_one(page_table, vpn) {
                Ok(_) => {
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            };
        }
        Ok(())
    }

    #[allow(unused)]
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }

    pub fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
}

// page_table: all pte of current MemorySet
// areas: all physical frames where current data stored
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}

impl MemorySet {
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }

    pub fn from_existed_userspace(user_space: &Self) -> Self {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // copy data from existed user space
        for area in user_space.areas.iter() {
            memory_set.push(MapArea::from_another(&area), None).unwrap(); // TODO assume that memory sapce is enough
            for vpn in area.vpn_range {
                let src_ppn = user_space.translate(vpn).unwrap().ppn();
                let dst_ppn = memory_set.translate(vpn).unwrap().ppn();
                dst_ppn
                    .get_bytes_array()
                    .copy_from_slice(src_ppn.get_bytes_array());
            }
        }
        memory_set
    }

    pub fn token(&self) -> usize {
        self.page_table.token()
    }

    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }

    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            satp::write(satp);
            asm!("sfence.vma");
        }
    }

    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) -> Result<(), &'static str> {
        match map_area.map(&mut self.page_table) {
            Ok(_) => {
                if let Some(data) = data {
                    map_area.copy_data(&mut self.page_table, data);
                }
                self.areas.push(map_area);
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn remove_area_with_start_vpn(&mut self, start_vpn: VirtPageNum) {
        if let Some((idx, area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_, area)| area.vpn_range.get_start() == start_vpn)
        {
            area.unmap(&mut self.page_table);
            self.areas.remove(idx);
        }
    }

    // TODO improve bruteforce munmap
    pub fn remove_mapped_frames(&mut self, start_va: VirtAddr, end_va: VirtAddr) -> isize {
        // make sure the vpn is belong to current MemorySet
        for vpn in VPNRange::new(start_va.floor(), end_va.ceil()) {
            if let None = self
                .areas
                .iter()
                .position(|area| area.data_frames.contains_key(&vpn))
            {
                return -1;
            }
        }
        // drop the MapAreas in a bruteforce way
        for vpn in VPNRange::new(start_va.floor(), end_va.ceil()) {
            let index = self
                .areas
                .iter()
                .position(|area| area.data_frames.contains_key(&vpn))
                .unwrap();
            self.areas[index].unmap_one(&mut self.page_table, vpn);
            self.areas[index].data_frames.remove(&vpn);
            if self.areas[index].data_frames.is_empty() {
                self.areas.remove(index);
            }
        }
        0
    }

    pub fn recycle_data_pages(&mut self) {
        self.areas.clear();
    }

    /// Assume that no conflicts.
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) -> Result<(), &'static str> {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        )
    }

    // assume that space is enough
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as usize).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }

    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();
        memory_set.map_trampoline();
        // map kernel sections
        info!(
            "mapping .text section [{:#x}, {:#x})",
            stext as usize, etext as usize
        );
        memory_set
            .push(
                MapArea::new(
                    (stext as usize).into(),
                    (etext as usize).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::X,
                ),
                None,
            )
            .unwrap();
        info!(
            "mapping .rodata section [{:#x}, {:#x})",
            srodata as usize, erodata as usize
        );
        memory_set
            .push(
                MapArea::new(
                    (srodata as usize).into(),
                    (erodata as usize).into(),
                    MapType::Identical,
                    MapPermission::R,
                ),
                None,
            )
            .unwrap();
        info!(
            "mapping .data section [{:#x}, {:#x})",
            sdata as usize, edata as usize
        );
        memory_set
            .push(
                MapArea::new(
                    (sdata as usize).into(),
                    (edata as usize).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            )
            .unwrap();
        info!(
            "mapping .bss section [{:#x}, {:#x})",
            sbss_with_stack as usize, ebss as usize
        );
        memory_set
            .push(
                MapArea::new(
                    (sbss_with_stack as usize).into(),
                    (ebss as usize).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            )
            .unwrap();
        // Physical memory identical map
        info!(
            "mapping physical memory [{:#x}, {:#x})",
            ekernel as usize, MEMORY_END as usize
        );
        memory_set
            .push(
                MapArea::new(
                    (ekernel as usize).into(),
                    MEMORY_END.into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            )
            .unwrap();
        info!("mapping memory-mapped registers");
        for pair in MMIO {
            memory_set
                .push(
                    MapArea::new(
                        pair.0.into(),
                        (pair.0 + pair.1).into(),
                        MapType::Identical,
                        MapPermission::R | MapPermission::W,
                    ),
                    None,
                )
                .ok();
        }
        memory_set
    }
    // Include sections in elf, set trampoline and TrapContext and user stack,
    // also returns user_sp and entry point.
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_vpn = map_area.vpn_range.get_end();
                memory_set
                    .push(
                        map_area,
                        Some(
                            &elf.input
                                [ph.offset() as usize..(ph.offset() + ph.file_size()) as usize],
                        ),
                    )
                    .unwrap();
            }
        }
        // map user stack with U flags
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_base: usize = max_end_va.into();
        // guard page
        user_stack_base += PAGE_SIZE;
        (
            memory_set,
            user_stack_base,
            elf.header.pt2.entry_point() as usize,
        )
    }
}

lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> =
        Arc::new(unsafe { UPSafeCell::new(MemorySet::new_kernel()) });
}

pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();
    assert!(!kernel_space
        .page_table
        .translate(mid_text.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_rodata.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_data.floor())
        .unwrap()
        .executable(),);
    debug!("remap_test passed!");
}
