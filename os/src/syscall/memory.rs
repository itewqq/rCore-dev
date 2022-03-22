use crate::config::PAGE_SIZE;
use crate::mm::{
    PageTable,
    VirtAddr, 
    MapPermission,
    VPNRange,
};
use crate::task::{
    current_user_token, 
    current_memory_set_mmap, 
    current_memory_set_munmap,
    current_id,
};

pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    if (start & (PAGE_SIZE - 1)) != 0 
        || (prot & !0x7) != 0
        || (prot & 0x7) == 0 {
        return -1;
    }

    let len = ( (len + PAGE_SIZE - 1) / PAGE_SIZE ) * PAGE_SIZE;
    let start_vpn =  VirtAddr::from(start).floor();
    let end_vpn =  VirtAddr::from(start + len).ceil();
    
    let page_table_user = PageTable::from_token(current_user_token());
    // make sure there are no mapped pages in [start..start+len)
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        if let Some(_) = page_table_user.translate(vpn) {
            return -1;
        }
    }
    let mut map_perm = MapPermission::U;
    if (prot & 0x1) != 0 {
        map_perm |= MapPermission::R;
    }
    if (prot & 0x2) !=0 {
        map_perm |= MapPermission::W;
    }
    if (prot & 0x4) !=0 {
        map_perm |= MapPermission::X;
    }

    match current_memory_set_mmap(
        VirtAddr::from(start), 
        VirtAddr::from(start + len), 
        map_perm) {
            Ok(_) => 0,
            Err(e) => {
                error!("[Kernel]: mmap error {}, task id={}", e, current_id());
                -1
            }
    }
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    if (start & (PAGE_SIZE - 1)) != 0 {
        return -1;
    }

    let len = ( (len + PAGE_SIZE - 1) / PAGE_SIZE ) * PAGE_SIZE;
    let start_vpn =  VirtAddr::from(start).floor();
    let end_vpn =  VirtAddr::from(start + len).ceil();

    let page_table_user = PageTable::from_token(current_user_token());
    // make sure there are no unmapped pages in [start..start+len)
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        if let None = page_table_user.translate(vpn) {
            return -1;
        }
    }
    
    current_memory_set_munmap( VirtAddr::from(start), VirtAddr::from(start + len))
}