use memmap2::MmapMut;
use std::io;

use crate::units::MB;

// `mb` megabytes straight from the OS, every page touched: a plain allocation may not be committed
// until it is used, and the allocator may keep the memory after it is freed. A mapping
// goes back to the OS the moment it is dropped. Blocks while the pages are written.
pub fn hold(mb: i64) -> io::Result<MmapMut> {
    let mut map = MmapMut::map_anon(mb as usize * MB)?;
    map.fill(1);
    Ok(map)
}
