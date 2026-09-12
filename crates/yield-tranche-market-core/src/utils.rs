use crate::fixed::Percentage;

pub fn read_u8(bytes: &[u8], offset: usize) -> Option<u8> {
    bytes.get(offset).copied()
}

pub fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let value = bytes.get(offset..offset.checked_add(2)?)?;
    // SAFETY: `value` contains exactly two initialized bytes. `read_unaligned`
    // permits the byte slice's arbitrary alignment.
    Some(u16::from_le(unsafe {
        core::ptr::read_unaligned(value.as_ptr().cast::<u16>())
    }))
}

pub fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let value = bytes.get(offset..offset.checked_add(4)?)?;
    // SAFETY: `value` contains exactly four initialized bytes. `read_unaligned`
    // permits the byte slice's arbitrary alignment.
    Some(u32::from_le(unsafe {
        core::ptr::read_unaligned(value.as_ptr().cast::<u32>())
    }))
}

pub fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let value = bytes.get(offset..offset.checked_add(8)?)?;
    // SAFETY: `value` contains exactly eight initialized bytes. `read_unaligned`
    // permits the byte slice's arbitrary alignment.
    Some(u64::from_le(unsafe {
        core::ptr::read_unaligned(value.as_ptr().cast::<u64>())
    }))
}

pub fn read_u128(bytes: &[u8], offset: usize) -> Option<u128> {
    let value = bytes.get(offset..offset.checked_add(16)?)?;
    // SAFETY: `value` contains exactly sixteen initialized bytes. `read_unaligned`
    // permits the byte slice's arbitrary alignment.
    Some(u128::from_le(unsafe {
        core::ptr::read_unaligned(value.as_ptr().cast::<u128>())
    }))
}

pub fn read_i64(bytes: &[u8], offset: usize) -> Option<i64> {
    let value = bytes.get(offset..offset.checked_add(8)?)?;
    // SAFETY: `value` contains exactly eight initialized bytes. `read_unaligned`
    // permits the byte slice's arbitrary alignment.
    Some(i64::from_le(unsafe {
        core::ptr::read_unaligned(value.as_ptr().cast::<i64>())
    }))
}

pub fn read_percentage(bytes: &[u8], offset: usize) -> Option<Percentage> {
    read_u16(bytes, offset).map(Percentage::new)
}
