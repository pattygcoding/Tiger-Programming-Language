//! A reference-counted string that fits in a pointer.
//!
//! `Rc<str>` is a fat pointer — a pointer and a length — so a `Value` holding one is
//! 24 bytes and every move of one moves 24. The length of a string is already stored
//! in the allocation that holds it, so this keeps a pointer to the bytes and reads the
//! length back from a header in front of them. That is the same single allocation and
//! the same single indirection `Rc<str>` has, in eight bytes instead of sixteen, which
//! is what lets a `Value` be 16 bytes.
//!
//! The handle also has room for the text itself. Allocations are eight-aligned, so the
//! low bit of the pointer is always clear, and setting it marks a handle whose eight
//! bytes *are* the string: a length in the first byte and up to seven bytes of text
//! after it. Programs build far more tiny strings than large ones — a dictionary key, a
//! character read out of a string, the result of `str(1)` — and those never reach the
//! allocator. A handle is copied rather than counted when it is inline, because the
//! text is part of it.
//!
//! Nothing here is reachable from safe code except through the methods below, and the
//! invariants they rest on are the `SAFETY` comments on the four places that read or
//! write a representation.

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::cell::Cell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::ptr::NonNull;

/// What sits in front of the bytes: the reference count and the length.
///
/// `#[repr(C)]` so the offset of the bytes is computed the same way everywhere, and
/// `usize`-aligned so the whole allocation is.
#[repr(C)]
struct Header {
    refs: Cell<usize>,
    len: usize,
    /// How much text the allocation has room for. `len` is how much of it is used;
    /// the two differ only after an append that grew the buffer.
    capacity: usize,
}

/// How many bytes of the allocation the header takes, and so where the text starts.
const BYTES: usize = std::mem::size_of::<Header>();

/// How many bytes a handle is, and so how much text one can hold inline: a length byte
/// and the rest.
const WIDTH: usize = std::mem::size_of::<usize>();
const INLINE_MAX: usize = WIDTH - 1;

/// The largest count that can be incremented without wrapping. `Rc` uses the same
/// bound, for the same reason: no program can reach it, and reaching it would mean a
/// count that no longer tracks the clones.
const MAX_REFS: usize = isize::MAX as usize;

/// The layout of a string whose buffer holds `capacity` bytes: the header, then the text.
fn layout_for(capacity: usize) -> Layout {
    let size = BYTES.checked_add(capacity).expect("a string is never too long to allocate");
    Layout::from_size_align(size, std::mem::align_of::<Header>()).expect("a string layout")
}

/// A shared, immutable string, eight bytes wide.
///
/// A heap string bumps a count on clone and decrements it on drop, exactly as `Rc`
/// does, and the allocation is freed when the last handle goes; a `&str` taken from one
/// stays valid for as long as the handle it came from, because the bytes change only in
/// [`RcStr::append`] and only when the count is one. An inline string is not shared at
/// all — its text is part of the handle, so a clone copies it and a drop does nothing.
///
/// Like `Rc` it is neither `Send` nor `Sync`.
pub struct RcStr {
    /// A pointer to the text with its low bit clear, or the text itself with the low
    /// bit set: a length in the first byte, then up to seven bytes of it.
    bytes: NonNull<u8>,
}

impl RcStr {
    pub fn new(text: &str) -> RcStr {
        RcStr::with_capacity(text.len(), text)
    }

    /// A string holding `text`, with room for `capacity` bytes so an append can grow
    /// it in place. A `capacity` below the text's own length is raised to it, and a
    /// string that fits in the handle is kept there instead of being allocated.
    fn with_capacity(capacity: usize, text: &str) -> RcStr {
        let len = text.len();
        let capacity = capacity.max(len);
        if capacity <= INLINE_MAX {
            return RcStr::inline(text);
        }
        // The heap branch is entered only when the allocation must hold more than the handle can,
        // and every caller fills it before the string becomes observable: `new` passes the whole
        // text, `append` grows to a total over the limit (or it would have stayed inline) and
        // writes that length before the handle is replaced, and `concat` sizes for both halves and
        // then appends the second. So a `RcStr` a caller can see is either an inline handle or an
        // allocation of more than seven bytes, and `len()` or `refs()` alone says which.
        let layout = layout_for(capacity);
        // SAFETY: the layout is never zero-sized, so a null return means the
        // allocator failed, and `handle_alloc_error` does not return.
        let base = unsafe { alloc(layout) };
        if base.is_null() {
            handle_alloc_error(layout);
        }
        // SAFETY: `base` is a fresh allocation of exactly `layout`, which is aligned
        // for `Header` and at least `BYTES + capacity` bytes, and `text` is a valid
        // `&str` whose `len` bytes are readable.
        unsafe {
            base.cast::<Header>().write(Header { refs: Cell::new(1), len, capacity });
            std::ptr::copy_nonoverlapping(text.as_ptr(), base.add(BYTES), len);
            RcStr { bytes: NonNull::new_unchecked(base.add(BYTES)) }
        }
    }

    /// A string whose text lives in the handle itself, for text that fits.
    fn inline(text: &str) -> RcStr {
        debug_assert!(text.len() <= INLINE_MAX, "an inline string has to fit");
        let mut handle = RcStr { bytes: NonNull::dangling() };
        let storage = handle.inline_storage_mut();
        storage[0] = (text.len() << 1) as u8 | 1;
        storage[1..1 + text.len()].copy_from_slice(text.as_bytes());
        handle
    }

    /// A string holding `value`'s digits, when `value` is an integer whose text
    /// fits in the handle, or `None`.
    ///
    /// The rendering is exactly [`tiger_format::number`]'s for an integral value:
    /// its `i64` form, with a negative zero keeping its sign. A caller that would
    /// otherwise render into a `String` and then copy that text into a shared
    /// string writes the digits once instead, which is what `str(i)` in a counting
    /// loop pays for.
    ///
    /// A handle holds seven bytes, so the text is at most seven digits, or a sign
    /// and six. Anything wider goes to the general formatter, which is the only
    /// place that knows how a float that is not an integer prints.
    pub fn from_integer(value: f64) -> Option<RcStr> {
        // A value that is not an integer, and a NaN or an infinity, all fail this:
        // `fract` of a NaN is a NaN, which is not zero.
        if value.fract() != 0.0 {
            return None;
        }
        let negative = value.is_sign_negative();
        let limit = if negative { 999_999.0 } else { 9_999_999.0 };
        if value.abs() > limit {
            return None;
        }
        // The digits are written into a stack buffer first, so the length is known
        // before anything is committed to the handle.
        let mut digits = [0u8; INLINE_MAX];
        let mut at = INLINE_MAX;
        let mut magnitude = value.abs() as u64;
        loop {
            at -= 1;
            digits[at] = b'0' + (magnitude % 10) as u8;
            magnitude /= 10;
            if magnitude == 0 {
                break;
            }
        }
        if negative {
            at -= 1;
            digits[at] = b'-';
        }
        Some(RcStr::inline(std::str::from_utf8(&digits[at..]).expect("digits are ASCII")))
    }

    /// Whether the text lives in the handle rather than in an allocation.
    fn is_inline(&self) -> bool {
        self.bytes.as_ptr() as usize & 1 == 1
    }

    /// The handle's own eight bytes, which hold an inline string.
    fn inline_storage(&self) -> &[u8; WIDTH] {
        // SAFETY: a handle is exactly `WIDTH` bytes and every bit pattern is a valid
        // `u8`, so viewing it as that many bytes is sound. The borrow ties the view to
        // the handle, which is what keeps a `&str` made from it alive.
        unsafe { &*(self as *const RcStr as *const [u8; WIDTH]) }
    }

    /// The same, for the one method that writes an inline string in place.
    fn inline_storage_mut(&mut self) -> &mut [u8; WIDTH] {
        // SAFETY: as above, and the exclusive borrow makes this the only view of it.
        unsafe { &mut *(self as *mut RcStr as *mut [u8; WIDTH]) }
    }

    /// The header this string's text follows.
    fn header(&self) -> &Header {
        // SAFETY: `bytes` points just past a live `Header` in the same allocation,
        // and the reference borrows that allocation through `self`.
        unsafe { &*self.bytes.as_ptr().sub(BYTES).cast::<Header>() }
    }

    /// The same header, for the one method that has exclusive access to it.
    fn header_mut(&mut self) -> &mut Header {
        // SAFETY: `self` is borrowed exclusively and the caller has checked that the
        // count is one, so this handle is the only one to the allocation.
        unsafe { &mut *self.bytes.as_ptr().sub(BYTES).cast::<Header>() }
    }

    pub fn len(&self) -> usize {
        if self.is_inline() {
            return (self.inline_storage()[0] >> 1) as usize;
        }
        self.header().len
    }

    /// How much text the buffer has room for without reallocating. An inline string's
    /// buffer is the rest of its own handle.
    pub fn capacity(&self) -> usize {
        if self.is_inline() {
            return INLINE_MAX;
        }
        self.header().capacity
    }

    /// Whether two handles share one allocation, and so the same text.
    ///
    /// A heap string's bytes never change through an alias — only the handle that is the
    /// last one can append in place — so equal pointers mean equal text. An inline
    /// string has no allocation to share: its bytes are its text, so two handles compare
    /// equal exactly when they hold the same characters.
    pub fn ptr_eq(left: &RcStr, right: &RcStr) -> bool {
        left.bytes == right.bytes
    }

    /// Appends `text`, growing the buffer in place when this is the only handle to the
    /// string and the buffer has room. A string that is still shared is left alone and
    /// a fresh one is built instead, so a binding that aliases it keeps its own text.
    ///
    /// Growth doubles the buffer, so a loop that appends to an accumulator costs the
    /// sum of its parts rather than the square of them.
    pub fn append(&mut self, text: &str) {
        let len = self.len();
        let total = len.checked_add(text.len()).expect("a string is never too long to append to");
        if self.is_inline() {
            // The text is part of this handle, so appending is always in place, and it
            // stays inline while the result still fits. A longer result falls through to
            // the heap, like any other growth.
            if total <= INLINE_MAX {
                let storage = self.inline_storage_mut();
                storage[1 + len..1 + total].copy_from_slice(text.as_bytes());
                storage[0] = (total << 1) as u8 | 1;
                return;
            }
        } else if total <= self.capacity() && self.header().refs.get() == 1 {
            // SAFETY: the count is one, so this is the only handle to the allocation
            // and no reader can observe the write, and the buffer has room for `total`
            // bytes starting at `bytes`.
            unsafe {
                std::ptr::copy_nonoverlapping(text.as_ptr(), self.bytes.as_ptr().add(len), text.len());
            }
            self.header_mut().len = total;
            return;
        }
        let grown = total.saturating_add(total / 2).max(total);
        let mut fresh = RcStr::with_capacity(grown, self.as_str());
        // SAFETY: `fresh` was just allocated with room for `grown >= total` bytes, and
        // its own text occupies the first `len` of them.
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), fresh.bytes.as_ptr().add(len), text.len());
        }
        fresh.header_mut().len = total;
        *self = fresh;
    }

    /// A string holding `first` followed by `second`, allocated once.
    ///
    /// The pair is sized up front, so `second` lands in the buffer `first` was copied into, and
    /// a result short enough to live in the handle is never allocated at all. Building it as a
    /// `String` and then copying that into an `RcStr` is two allocations and two copies of the
    /// same bytes.
    pub fn concat(first: &str, second: &str) -> RcStr {
        let total = first.len().checked_add(second.len()).expect("a string is never too long to concatenate");
        let mut joined = RcStr::with_capacity(total, first);
        joined.append(second);
        joined
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_str(&self) -> &str {
        if self.is_inline() {
            // SAFETY: the text was copied from a `&str` and lives in this handle, whose
            // borrow keeps it alive; nothing can write to it except `append`, which
            // needs exclusive access to the handle.
            return unsafe {
                let storage = self.inline_storage();
                let len = (storage[0] >> 1) as usize;
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(storage.as_ptr().add(1), len))
            };
        }
        // SAFETY: the bytes were copied from a `&str` and nothing can write to them
        // afterwards, so they are `len` bytes of valid UTF-8.
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes.as_ptr(), self.len())) }
    }

    /// The number of live handles, so a test can check the accounting directly. An
    /// inline string is not counted at all — its text is part of the handle — so this
    /// reports zero for one.
    #[cfg(test)]
    fn refs(&self) -> usize {
        if self.is_inline() {
            return 0;
        }
        self.header().refs.get()
    }
}

impl Clone for RcStr {
    #[inline]
    fn clone(&self) -> RcStr {
        if self.is_inline() {
            // The text is part of the handle, so a clone copies it: there is no count
            // to bump and nothing shared.
            return RcStr { bytes: self.bytes };
        }
        let refs = &self.header().refs;
        let count = refs.get();
        if count > MAX_REFS {
            // A count this high cannot be reached without leaking that many clones
            // first, so this is unreachable in practice. `Rc` aborts here as well.
            std::process::abort();
        }
        refs.set(count + 1);
        RcStr { bytes: self.bytes }
    }
}

impl Drop for RcStr {
    #[inline]
    fn drop(&mut self) {
        if self.is_inline() {
            // Nothing was allocated and nothing is counted.
            return;
        }
        let refs = &self.header().refs;
        let count = refs.get() - 1;
        if count > 0 {
            refs.set(count);
            return;
        }
        // Read the capacity before freeing, so nothing touches the header afterwards:
        // the allocation is the buffer's size, not the text's.
        let layout = layout_for(self.header().capacity);
        // SAFETY: the count just reached zero, so this was the last `RcStr` for the
        // allocation, which `RcStr::new` made with exactly this layout and which
        // nothing has freed yet.
        unsafe { dealloc(self.bytes.as_ptr().sub(BYTES), layout) };
    }
}

impl Deref for RcStr {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for RcStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for RcStr {
    fn eq(&self, other: &RcStr) -> bool {
        // Comparing the bytes, not the handles: equal text can be held two ways (in a handle and
        // in an allocation), and an inline fast path was measured and did not earn its branches —
        // see "A string key's hash is not what a dictionary operation pays for" in the guide.
        self.as_str() == other.as_str()
    }
}

impl Eq for RcStr {}

impl PartialOrd for RcStr {
    fn partial_cmp(&self, other: &RcStr) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RcStr {
    fn cmp(&self, other: &RcStr) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for RcStr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The bytes, so that the text is what a hasher sees. Hashing an inline handle as one word
        // instead was measured and did not earn its branches: a key's hash is a `write` plus a
        // terminator either way, and the fixed cost of the hash dominates both — see
        // "A string key's hash is not what a dictionary operation pays for" in the guide.
        self.as_str().hash(state)
    }
}

impl fmt::Debug for RcStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for RcStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for RcStr {
    fn from(text: &str) -> RcStr {
        RcStr::new(text)
    }
}

impl From<String> for RcStr {
    fn from(text: String) -> RcStr {
        RcStr::new(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    fn hash_of<T: Hash + ?Sized>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn it_is_one_pointer_wide() {
        assert_eq!(std::mem::size_of::<RcStr>(), std::mem::size_of::<usize>());
    }

    #[test]
    fn it_holds_the_text_and_its_length() {
        for text in ["", "a", "hello, world", "héllo", "日本語", "🎉 emoji"] {
            let held = RcStr::new(text);
            assert_eq!(held.as_str(), text);
            assert_eq!(held.len(), text.len());
            assert_eq!(held.is_empty(), text.is_empty());
            assert_eq!(&*held, text);
        }
    }

    #[test]
    fn a_small_integer_is_written_into_the_handle() {
        // The handle holds seven bytes, so seven digits fit, or a sign and six.
        let fits = [
            (0.0, "0"),
            (7.0, "7"),
            (50.0, "50"),
            (999_999.0, "999999"),
            (1_000_000.0, "1000000"),
            (9_999_999.0, "9999999"),
            (-1.0, "-1"),
            (-999_999.0, "-999999"),
            (-0.0, "-0"),
        ];
        for (value, expected) in fits {
            let text = RcStr::from_integer(value).unwrap_or_else(|| panic!("{value} should fit in the handle"));
            assert_eq!(text.as_str(), expected);
            // The rendering is the one the general formatter gives an integral value,
            // which is what makes the fast path invisible to a program.
            assert_eq!(text.as_str(), tiger_format::number(value));
            assert!(text.is_inline(), "{value} should be held in the handle");
            assert_eq!(text.len(), expected.len());
        }
        // A magnitude too wide for the handle, and anything that is not an integer at
        // all, is left to the general formatter — the only place that knows how a
        // value that is not an integer prints.
        for value in [-1_000_000.0, 10_000_000.0, 9_999_999.5, 1.5, -1.5, 1e21, 2f64.powi(53), f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(RcStr::from_integer(value).is_none(), "{value} should not fit in the handle");
        }
    }

    #[test]
    fn the_count_tracks_every_clone() {
        let held = RcStr::new("shared text");
        assert_eq!(held.refs(), 1);
        let second = held.clone();
        assert_eq!(held.refs(), 2);
        let third = second.clone();
        assert_eq!(third.refs(), 3);
        drop(third);
        assert_eq!(held.refs(), 2);
        drop(second);
        assert_eq!(held.refs(), 1);
        assert_eq!(held.as_str(), "shared text");
    }

    #[test]
    fn a_clone_outlives_the_original() {
        let held = RcStr::new("outlives");
        let kept = held.clone();
        drop(held);
        assert_eq!(kept.as_str(), "outlives");
        assert_eq!(kept.refs(), 1);
    }

    #[test]
    fn a_long_string_round_trips() {
        let text = "abc123".repeat(20_000);
        let held = RcStr::new(&text);
        assert_eq!(held.len(), 120_000);
        assert_eq!(held.as_str(), text.as_str());
    }

    #[test]
    fn many_short_strings_survive_their_own_lifetimes() {
        for round in 0..1_000 {
            let expected = format!("value {round}");
            let held = RcStr::new(&expected);
            let copy = held.clone();
            drop(held);
            assert_eq!(copy.as_str(), expected.as_str());
        }
    }

    #[test]
    fn it_compares_hashes_and_orders_like_a_str() {
        let a = RcStr::new("apple");
        let b = RcStr::new("banana");
        assert_eq!(a, RcStr::new("apple"));
        assert_ne!(a, b);
        assert!(a < b);
        assert_eq!(a.cmp(&b), "apple".cmp("banana"));
        // Equal strings hash equal, whether the text lives in the handle or in an allocation, and
        // the hash the standard hasher gives a `str` is the one they agree on.
        assert_eq!(hash_of(&a), hash_of("apple"));
        assert_eq!(hash_of(&RcStr::new("banana")), hash_of("banana"));
        assert_ne!(hash_of(&a), hash_of(&b));

        // A text too long for a handle hashes as itself too.
        let long = "a text too long for a handle";
        assert_eq!(hash_of(&RcStr::new(long)), hash_of(long));
    }

    #[test]
    fn an_inline_handle_and_an_allocation_are_never_the_same_text() {
        // The two representations divide the lengths between them: up to seven bytes live in the
        // handle and an allocation is always longer, so `len()` and `refs()` alone say which
        // representation a string is using.
        let held = RcStr::new("1234567");
        assert_eq!(held.refs(), 0);
        assert_eq!(held.as_str().len(), INLINE_MAX);
        let allocated = RcStr::new("12345678");
        assert_eq!(allocated.as_str().len(), INLINE_MAX + 1);
        assert!(allocated.refs() >= 1);
        assert_ne!(held, allocated);
        assert_ne!(hash_of(&held), hash_of(&allocated));

        // A string that grows out of the handle is an allocation afterwards, and still compares
        // and hashes as the same text it was before.
        let mut grown = RcStr::new("1234567");
        grown.append("8");
        assert_eq!(grown.as_str(), "12345678");
        assert_eq!(grown, RcStr::new("12345678"));
        assert_eq!(hash_of(&grown), hash_of(&RcStr::new("12345678")));

        // Two equal allocations compare equal without sharing one, which is what the second
        // half of `eq` is for.
        let first = RcStr::new("an allocated text");
        let second = RcStr::new("an allocated text");
        assert_ne!(RcStr::ptr_eq(&first, &second), RcStr::ptr_eq(&first, &first));
        assert_eq!(first, second);
        assert_eq!(hash_of(&first), hash_of(&second));
    }

    #[test]
    fn a_concatenation_allocates_once() {
        let joined = RcStr::concat("hello, ", "world");
        assert_eq!(joined.as_str(), "hello, world");
        assert_eq!(joined.refs(), 1);
        assert_eq!(joined.capacity(), 12);
        // A result short enough for the handle is not allocated and not counted at all.
        let small = RcStr::concat("k", "12");
        assert_eq!(small.as_str(), "k12");
        assert_eq!(small.refs(), 0);
        assert_eq!(RcStr::concat("", "").as_str(), "");
        assert_eq!(RcStr::concat("", "abc").as_str(), "abc");
        assert_eq!(RcStr::concat("abc", "").as_str(), "abc");
        // The result is its own string, independent of the halves it was built from.
        let first = RcStr::new("shared ");
        let joined = RcStr::concat(&first, "tail");
        assert_eq!(joined.as_str(), "shared tail");
        assert_eq!(joined.refs(), 1);
        drop(first);
        assert_eq!(joined.as_str(), "shared tail");
    }

    #[test]
    fn it_formats_like_a_str() {
        let held = RcStr::new("formatted");
        assert_eq!(format!("{}", held), "formatted");
        assert_eq!(format!("{:?}", held), "\"formatted\"");
    }

    #[test]
    fn from_a_string_matches_from_a_str() {
        let owned = RcStr::from(String::from("owned"));
        let borrowed = RcStr::from("owned");
        assert_eq!(owned, borrowed);
        assert_eq!(owned.as_str(), borrowed.as_str());
    }

    #[test]
    fn appending_reuses_the_buffer_when_it_is_the_only_handle() {
        let mut held = RcStr::new("abcdefgh");
        assert_eq!(held.capacity(), 8);
        held.append("ijk");
        assert_eq!(held.as_str(), "abcdefghijk");
        let capacity = held.capacity();
        assert!(capacity >= 11);
        // This one fits in the room the growth left, so the buffer does not move.
        held.append("l");
        assert_eq!(held.as_str(), "abcdefghijkl");
        assert_eq!(held.capacity(), capacity);
        assert_eq!(held.refs(), 1);
    }

    #[test]
    fn a_short_string_lives_in_the_handle() {
        for text in ["", "a", "ab", "abc", "abcd", "abcde", "abcdef", "abcdefg"] {
            let held = RcStr::new(text);
            assert_eq!(held.len(), text.len());
            assert_eq!(held.as_str(), text);
            assert_eq!(&*held, text);
            assert_eq!(held.refs(), 0, "an inline string is not counted");
        }
    }

    #[test]
    fn a_string_past_the_handle_is_allocated() {
        let held = RcStr::new("abcdefgh");
        assert_eq!(held.as_str(), "abcdefgh");
        assert_eq!(held.len(), 8);
        assert_eq!(held.capacity(), 8);
        assert_eq!(held.refs(), 1);
    }

    #[test]
    fn appending_keeps_a_short_string_inline() {
        let mut held = RcStr::new("abc");
        assert_eq!(held.refs(), 0);
        held.append("de");
        assert_eq!(held.as_str(), "abcde");
        assert_eq!(held.refs(), 0, "it stayed inline");
        // And one that no longer fits moves to the heap.
        held.append("fgh");
        assert_eq!(held.as_str(), "abcdefgh");
        assert_eq!(held.len(), 8);
        assert_eq!(held.refs(), 1);
    }

    #[test]
    fn an_inline_copy_has_its_own_text() {
        let mut held = RcStr::new("abc");
        let copy = held.clone();
        held.append("de");
        assert_eq!(held.as_str(), "abcde");
        assert_eq!(copy.as_str(), "abc", "the copy is not shared");
    }

    #[test]
    fn appending_to_a_shared_string_leaves_the_other_handle_alone() {
        let mut held = RcStr::new("shared");
        // Grow once so there is room to spare, which is when writing in place would
        // be tempting and wrong.
        held.append(" room");
        assert_eq!(held.as_str(), "shared room");
        assert!(held.capacity() > held.len());
        let kept = held.clone();
        held.append("!");
        assert_eq!(held.as_str(), "shared room!");
        assert_eq!(kept.as_str(), "shared room");
        // The append built a fresh buffer rather than writing through the alias.
        assert_eq!(held.refs(), 1);
        assert_eq!(kept.refs(), 1);
    }

    #[test]
    fn appending_an_empty_string_changes_nothing() {
        let mut held = RcStr::new("same");
        let capacity = held.capacity();
        held.append("");
        assert_eq!(held.as_str(), "same");
        assert_eq!(held.capacity(), capacity);
    }

    #[test]
    fn a_long_append_chain_matches_a_string_builder() {
        let mut held = RcStr::new("");
        let mut expected = String::new();
        for step in 0..2_000 {
            let piece = format!("{step},");
            held.append(&piece);
            expected.push_str(&piece);
        }
        assert_eq!(held.as_str(), expected.as_str());
        assert_eq!(held.len(), expected.len());
    }

    #[test]
    fn appending_unicode_keeps_it_valid() {
        let mut held = RcStr::new("héllo");
        held.append(" 日本語");
        held.append(" 🎉");
        assert_eq!(held.as_str(), "héllo 日本語 🎉");
        assert_eq!(held.len(), "héllo 日本語 🎉".len());
    }

    #[test]
    fn pointer_equality_follows_the_allocation() {
        let held = RcStr::new("same text");
        let alias = held.clone();
        let other = RcStr::new("same text");
        assert!(RcStr::ptr_eq(&held, &alias));
        assert!(!RcStr::ptr_eq(&held, &other));
        assert_eq!(held, other);
    }
}
