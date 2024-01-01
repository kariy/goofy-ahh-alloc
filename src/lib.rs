#![feature(allocator_api)]
#![feature(ptr_metadata)]

use dashmap::DashMap;
use lazy_static::lazy_static;
use memmap2::{MmapMut, MmapOptions};
use std::{
    alloc::{AllocError, Allocator, GlobalAlloc, Layout, System},
    fs::{remove_file, File, OpenOptions},
    path::PathBuf,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
};

struct FileMmapHandle {
    file: File,
    map: MmapMut,
    path: PathBuf,
}

static ALLOCATING: AtomicBool = AtomicBool::new(false);
static DEALLOCATING: AtomicBool = AtomicBool::new(false);

lazy_static! {
    static ref FILE_MEM_MAP: DashMap<usize, FileMmapHandle> = DashMap::with_capacity(1_000_000);
}

pub struct FileAllocator;

unsafe impl Allocator for FileAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // De/allocation that is performed during `GoofyAhhAllocator` de/allocation implementations are done using the system allocator
        //
        // This is done to prevent infinite recursion, because otheriwise the `GoofyAhhAllocator` would be used to allocate the
        // `GoofyAhhAllocator` itself, which would then caused itself to be used to allocate itself, and so on. Which would end up
        // in a stack overflow.
        if ALLOCATING.load(Ordering::SeqCst) || DEALLOCATING.load(Ordering::SeqCst) {
            System.allocate(layout)
        } else {
            ALLOCATING.store(true, Ordering::SeqCst);

            let file_path = get_alloc_file_path();
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&file_path)
                .unwrap();

            file.set_len(layout.size() as u64).unwrap();
            let map = unsafe { MmapOptions::new().map_mut(&file).unwrap() };

            let ptr =
                NonNull::from_raw_parts(NonNull::new(map.as_ptr() as _).unwrap(), layout.size());

            FILE_MEM_MAP.insert(
                ptr.as_ptr() as *mut u8 as usize,
                FileMmapHandle {
                    file,
                    map,
                    path: file_path,
                },
            );

            ALLOCATING.store(false, Ordering::SeqCst);
            Ok(ptr)
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // if we cant find the ptr in the map, then we it must have been allocated by the system allocator
        if let Some((_, FileMmapHandle { file, path, map })) =
            FILE_MEM_MAP.remove(&(ptr.as_ptr() as usize))
        {
            DEALLOCATING.store(true, Ordering::SeqCst);
            // Unmmapped the memory by dropping the `MmapMut` handle.
            drop(map);
            // Drop the file handle.
            drop(file);
            // Delete the file.
            let _ = remove_file(path).unwrap();

            DEALLOCATING.store(false, Ordering::SeqCst);
        } else {
            System.deallocate(ptr, layout);
        }
    }
}

fn get_alloc_file_path() -> PathBuf {
    use std::sync::atomic::AtomicU64;

    static ALLOC_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    std::env::temp_dir().join(format!(
        "goofy-alloc_{:010}.mem",
        ALLOC_FILE_COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

unsafe impl GlobalAlloc for FileAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = <Self as Allocator>::allocate(self, layout).unwrap();
        ptr.as_ptr() as _
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        <Self as Allocator>::deallocate(self, NonNull::new(ptr).unwrap(), layout)
    }
}
