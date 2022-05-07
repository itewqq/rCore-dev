use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};

use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};

pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// We should not acquire efs lock here.
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }

    pub fn inode_id(&self) -> u32 {
        self.read_disk_inode(|disk_node|{
            disk_node.inode_id
        })
    }

    pub fn nlink(&self) -> usize {
        self.read_disk_inode(|disk_node|{
            disk_node.nlink as usize
        })
    }

    // we only have two Inode type for now
    pub fn mode(&self) -> DiskInodeType {
        self.read_disk_inode(|disk_node|{
            if disk_node.is_dir() {
                DiskInodeType::Directory
            }else{
                DiskInodeType::File
            }
        })
    }

    pub fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }

    pub fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }

    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }

    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::new_zeros();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_number() as u32);
            }
        }
        None
    }

    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::new_zeros();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device),
                    DIRENT_SZ
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }

    // assume it can only be called by the root Inode
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        if self
            .read_disk_inode(|root_inode| {
                // assert it is a directory
                assert!(root_inode.is_dir());
                // has the file been created?
                self.find_inode_id(name, root_inode)
            })
            .is_some()
        {
            return None;
        }
        // create a new file, alloc a inode id
        let new_inode_id = fs.alloc_inode();
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        // initalize it with empty
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(new_inode_id, DiskInodeType::File);
            });
        // append file in the dirent
        self.modify_disk_inode(|root_inode| {
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });
        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        assert_eq!(
            (block_id, block_offset),
            (new_inode_block_id, new_inode_block_offset)
        );
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }

    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }

    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }

    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }

    // assume it can only be called by the root Inode
    pub fn linkat(&self, oldpath: &str, newpath: &str, flags: u32) -> isize {
        // for now just support AT_FDCWD
        assert_eq!(flags, 0);
        // if the newpath already exsist, return -1
        if self
            .read_disk_inode(|root_inode| {
                assert!(root_inode.is_dir());
                self.find_inode_id(newpath, root_inode)
            })
            .is_some()
        {
            return -1;
        }
        // if the oldpath not exist, return -1, otherwise hard link it with new path
        match self.read_disk_inode(|root_inode| {
            assert!(root_inode.is_dir());
            self.find_inode_id(oldpath, root_inode)
        }) {
            Some(old_inode_id) => {
                // make sure oldpath is a file
                self.find(oldpath)
                    .unwrap()
                    .read_disk_inode(|old_disk_node| {
                        assert_eq!(old_disk_node.is_file(), true);
                    });

                // update link number here for the fs.lock()
                self.find(oldpath).unwrap().modify_disk_inode(|disk_inode| {
                    disk_inode.inc_nlink();
                });
                let mut fs = self.fs.lock();
                let dirent = DirEntry::new(newpath, old_inode_id);
                self.modify_disk_inode(|root_inode| {
                    let file_count = (root_inode.size as usize) / DIRENT_SZ;
                    let new_size = (file_count + 1) * DIRENT_SZ;
                    // increase size
                    self.increase_size(new_size as u32, root_inode, &mut fs);
                    root_inode.write_at(
                        file_count * DIRENT_SZ,
                        dirent.as_bytes(),
                        &self.block_device,
                    );
                });
                // block_cache_sync_all();
                0
            }
            None => -1,
        }
    }

    // assume it can only be called by the root Inode
    pub fn unlinkat(&self, path: &str, flags: u32) -> isize {
        assert_eq!(flags, 0);
        match self.read_disk_inode(|root_inode| {
            assert!(root_inode.is_dir());
            self.find_inode_id(path, root_inode)
        }) {
            Some(_) => {
                // not delete the inode, nor free the data, just update the link number...
                // TODO implement the file delete function
                let mut dirent = DirEntry::new_zeros();
                let res = self.modify_disk_inode(|root_inode| {
                    let file_count = (root_inode.size as usize) / DIRENT_SZ;
                    for i in 0..file_count {
                        assert_eq!(
                            root_inode.read_at(
                                DIRENT_SZ * i,
                                dirent.as_bytes_mut(),
                                &self.block_device,
                            ),
                            DIRENT_SZ,
                        );
                        if dirent.name() == path {
                            // write a empty block to cover the old one
                            root_inode.write_at(
                                file_count * DIRENT_SZ,
                                DirEntry::new_zeros().as_bytes(),
                                &self.block_device,
                            );
                            return 0;
                        }
                    }
                    return -1;
                });
                if res == 0 {
                    // ==== update link number ====
                    let target_inode = self.find(path).unwrap();
                    target_inode.modify_disk_inode(|target_inode| {
                        target_inode.dec_nlink();
                    });
                }
                res
            }
            None => {
                // if the path not exsist, return -1
                -1
            }
        }
    }

    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
}
