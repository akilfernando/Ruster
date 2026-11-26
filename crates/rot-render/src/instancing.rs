//! Dirty-range GPU instance buffer upload tracker.
use super::Instance;
use bytemuck::Zeroable;

pub(crate) const MAX_CLEAN_GAP: usize = 4;

#[derive(Default)]
pub(crate) struct InstanceUploadTracker {
    shadow: Vec<Instance>,
}

impl InstanceUploadTracker {
    /// Drop cached GPU contents after a storage buffer reallocation.
    pub(crate) fn clear(&mut self) {
        self.shadow.clear();
    }

    /// Synchronize only modified instance slices to GPU memory, coalescing short clean gaps.
    /// Returns `(write_calls, bytes_written)`.
    pub(crate) fn sync<F: FnMut(u64, &[u8])>(
        &mut self,
        groups: &[(&[Instance], u32)],
        total: usize,
        mut write: F,
    ) -> (u64, u64) {
        let prev_len = self.shadow.len();
        self.shadow.truncate(total);
        if total > prev_len {
            self.shadow.resize(total, Instance::zeroed());
        }
        let mut writes = 0u64;
        let mut written_bytes = 0u64;
        for &(instances, start) in groups {
            if instances.is_empty() {
                continue;
            }
            let base = start as usize;
            let end = base + instances.len();
            debug_assert!(end <= self.shadow.len());
            if end <= prev_len
                && bytemuck::cast_slice::<Instance, u8>(instances)
                    == bytemuck::cast_slice::<Instance, u8>(&self.shadow[base..end])
            {
                continue;
            }
            let mut i = 0;
            while i < instances.len() {
                let dirty = base + i >= prev_len
                    || bytemuck::bytes_of(&instances[i])
                        != bytemuck::bytes_of(&self.shadow[base + i]);
                if !dirty {
                    i += 1;
                    continue;
                }
                let run_start = i;
                let mut run_end = i + 1;
                let mut clean_streak = 0usize;
                let mut scan = run_end;
                while scan < instances.len() {
                    let next_dirty = base + scan >= prev_len
                        || bytemuck::bytes_of(&instances[scan])
                            != bytemuck::bytes_of(&self.shadow[base + scan]);
                    if next_dirty {
                        run_end = scan + 1;
                        clean_streak = 0;
                    } else {
                        clean_streak += 1;
                        if clean_streak > MAX_CLEAN_GAP {
                            break;
                        }
                    }
                    scan += 1;
                }
                let dst = &mut self.shadow[base + run_start..base + run_end];
                dst.copy_from_slice(&instances[run_start..run_end]);
                let byte_offset = ((base + run_start) * size_of::<Instance>()) as u64;
                let bytes = bytemuck::cast_slice(dst);
                write(byte_offset, bytes);
                writes += 1;
                written_bytes += bytes.len() as u64;
                i = if scan < instances.len() {
                    scan + 1
                } else {
                    run_end
                };
            }
        }
        (writes, written_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(tag: f32) -> Instance {
        let mut inst = Instance::zeroed();
        inst.color = [tag, 1.0, 0.0, 1.0];
        inst
    }

    #[test]
    fn sync_skips_unchanged_and_tracks_dirty_runs() {
        let mut tracker = InstanceUploadTracker::default();
        let stride = size_of::<Instance>() as u64;
        let mut data: Vec<Instance> = (0..1000).map(|i| sample(i as f32)).collect();

        // 1. Initial upload writes all 1000 instances once.
        let mut ops = Vec::new();
        let (w1, b1) = tracker.sync(&[(&data, 0)], data.len(), |off, bytes| {
            ops.push((off, bytes.len() as u64));
        });
        assert_eq!(w1, 1);
        assert_eq!(b1, 1000 * stride);
        assert_eq!(ops, vec![(0, 1000 * stride)]);

        // 2. Identical frame uploads 0 bytes.
        ops.clear();
        let (w2, b2) = tracker.sync(&[(&data, 0)], data.len(), |off, bytes| {
            ops.push((off, bytes.len() as u64));
        });
        assert_eq!((w2, b2), (0, 0));
        assert!(ops.is_empty());

        // 3. Single mutated element uploads exact 1 * Instance at idx * stride.
        data[321].base_color = [0.5, 0.25, 0.75, 1.0];
        let (w3, b3) = tracker.sync(&[(&data, 0)], data.len(), |off, bytes| {
            ops.push((off, bytes.len() as u64));
        });
        assert_eq!((w3, b3), (1, stride));
        assert_eq!(ops, vec![(321 * stride, stride)]);

        // 4. Gap <= MAX_CLEAN_GAP coalesces; gap > MAX_CLEAN_GAP splits.
        ops.clear();
        data[10].color[0] = 9999.0;
        data[14].color[0] = 9998.0; // gap of 3 clean elements (11, 12, 13) <= 4
        data[21].color[0] = 9997.0; // gap of 6 clean elements (15..=20) > 4
        let (w4, b4) = tracker.sync(&[(&data, 0)], data.len(), |off, bytes| {
            ops.push((off, bytes.len() as u64));
        });
        assert_eq!(w4, 2);
        assert_eq!(b4, 6 * stride);
        assert_eq!(ops, vec![(10 * stride, 5 * stride), (21 * stride, stride)]);

        // 5. Shrink then regrow forces upload of regrown tail.
        ops.clear();
        assert_eq!(
            tracker.sync(&[(&data[..5], 0)], 5, |off, bytes| {
                ops.push((off, bytes.len() as u64));
            }),
            (0, 0)
        );
        let (w5, b5) = tracker.sync(&[(&data[..8], 0)], 8, |off, bytes| {
            ops.push((off, bytes.len() as u64));
        });
        assert_eq!((w5, b5), (1, 3 * stride));
        assert_eq!(ops, vec![(5 * stride, 3 * stride)]);

        // 6. clear() forces full re-upload on next sync.
        tracker.clear();
        ops.clear();
        let (w6, b6) = tracker.sync(&[(&data, 0)], data.len(), |off, bytes| {
            ops.push((off, bytes.len() as u64));
        });
        assert_eq!((w6, b6), (1, 1000 * stride));
    }
}
