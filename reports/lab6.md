# File system

In this lab we are asked to implement 3 syscalls, `sys_linkat`, `sys_unlinkat`, and `sys_stat`.

## linkat

The hard/simbolic link in Linux can be illustrated in the following figure:

![](https://miro.medium.com/max/1024/1*cO1xeJsWtEFrKwycHWcFqw.jpeg)

- The hard links share the exactly same Inode
- The symbolic links works like a shortcut in Windows, it is a special file and carries some data point to another file.

Once understood, it is easy to implement hard links:

```rust
    // assume it can only be called by the root Inode
    pub fn linkat(
        &self,
        olddirfd: i32,
        oldpath: &str,
        newdirfd: i32,
        newpath: &str,
        flags: u32,
    ) -> i32 {
        // for now just support AT_FDCWD
        assert_eq!(olddirfd, AT_FDCWD);
        assert_eq!(newdirfd, AT_FDCWD);
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
```

## unlink

Since we have introduced hard links, it's important to maintain the hardlink numbers of a file. 

First, we add a `nlink` filed and corresponding methods `inc_nlink/dec_nlink` in the `DiskInode` struct:

```rust
@@ -60,6 +60,7 @@ pub struct DiskInode {
    pub direct: [u32; INODE_DIRECT_COUNT],
    pub indirect1: u32,
    pub indirect2: u32,
    pub nlink: u32,
    type_: DiskInodeType,
}

@@ -70,9 +71,18 @@ impl DiskInode {
        self.direct.iter_mut().for_each(|v| *v = 0);
        self.indirect1 = 0;
        self.indirect2 = 0;
        self.nlink = 1;
        self.type_ = type_;
    }

    pub fn inc_nlink(&mut self){
        self.nlink += 1;
    }

    pub fn dec_nlink(&mut self){
        self.nlink -= 1;
    }

    pub fn is_dir(&self) -> bool {
        self.type_ == DiskInodeType::Directory
    }
```

Then we adjust the previous `linkat` code:

```rust
@@ -208,6 +210,11 @@ impl Inode {
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
@@ -228,6 +235,57 @@ impl Inode {
        }
    }
```

Finally, the `unlinkat` interface for Inode, which is basically the reversed version of `linkat`:

```rust
@@ -228,6 +235,57 @@ impl Inode {
        }
    }

    // assume it can only be called by the root Inode
    pub fn unlinkat(&self, dirfd: i32, path: &str, flags: u32) -> i32 {
        assert_eq!(dirfd, AT_FDCWD);
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
```

## fstat

First we make the types for `fstat`:

```rust
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    pub dev: u64,
    /// inode number
    pub ino: u64,
    /// file type and mode
    pub mode: StatMode,
    /// number of hard links
    pub nlink: u32,
    /// unused pad
    pad: [u64; 7],
}

impl Stat {
    pub fn new() -> Self {
        Stat {
            dev: 0,
            ino: 0,
            mode: StatMode::NULL,
            nlink: 0,
            pad: [0; 7],
        }
    }
}

impl Default for Stat {
    fn default() -> Self {
        Self::new()
    }
}

bitflags! {
    pub struct StatMode: u32 {
        const NULL  = 0;
        /// directory
        const DIR   = 0o040000;
        /// ordinary regular file
        const FILE  = 0o100000;
    }
}

```

Note that we already have all the interfaces for the fileds of the `Stat` structure, except `inode_id` and `mode`. So we just implement those interfaces for `Inode`:

```rust
pub struct Inode {
@@ -33,6 +31,29 @@ impl Inode {
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
```

Then the `fstat` for OSInode is naive:

```rust
@@ -107,6 +107,21 @@ impl File for OSInode {
        }
        total_write_size
    }

    fn fstat(&self) -> Stat {
        let inner = self.inner.exclusive_access();
        let mode = match inner.inode.mode() {
            DiskInodeType::File => StatMode::FILE,
            DiskInodeType::Directory => StatMode::DIR,
        };
        Stat {
            dev: 0,
            ino: inner.inode.inode_id() as u64,
            mode,
            nlink: inner.inode.nlink() as u32,
            pad: [0; 7],
        }
    }
```

## Add the syscalls

Finally, we add the syscalls to those interfaces:

```rust
pub fn sys_linkat(
    olddirfd: i32,
    oldpath: *const u8,
    newdirfd: i32,
    newpath: *const u8,
    flags: u32,
) -> isize {
    assert_eq!(olddirfd, AT_FDCWD);
    assert_eq!(newdirfd, AT_FDCWD);
    assert_eq!(flags, 0);
    let token = current_user_token();
    let oldpath = translated_str(token, oldpath);
    let newpath = translated_str(token, newpath);
    linkat(&oldpath, &newpath, flags)
}

pub fn sys_unlinkat(dirfd: i32, path: *const u8, flags: u32) -> isize {
    assert_eq!(dirfd, AT_FDCWD);
    assert_eq!(flags, 0);
    let token = current_user_token();
    let path = translated_str(token, path);
    unlinkat(&path, flags)
}

pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    // translate buffer in user mode
    let len = size_of::<Stat>();
    let mut ts_buffers = translated_byte_buffers(current_user_token(), st.cast(), len);
    // At least one buf
    if ts_buffers.len() <= 0 {
        return -1;
    }
    let st: *mut Stat = ts_buffers[0].as_mut_ptr().cast();

    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        let stat = file.fstat();
        unsafe {
            *st = stat;
        }
        0
    } else {
        -1
    }
}
```
