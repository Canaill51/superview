//! What the GPU paths share: a kernel built once per GPU context (DEC-66).

/// One value per context, built once; a build that fails is remembered for that context.
///
/// A kernel that does not load (a CUDA card older than `sm_50`, a driver too old for the PTX, an
/// OpenCL compiler that rejects the source) fails the same way on every frame:
/// retrying would compile it again each time for nothing. Another context (another GPU, a
/// restarted engine) gets one new try.
#[derive(Debug, Default)]
pub enum PerContext<C, T> {
    #[default]
    Empty,
    Ready(C, T),
    Failed(C, String),
}

impl<C: PartialEq + Copy, T> PerContext<C, T> {
    /// The value for `context`, built by `load` unless this context already has one or already
    /// failed to build one.
    pub fn get_or_load(
        &mut self,
        context: C,
        load: impl FnOnce() -> Result<T, String>,
    ) -> Result<&mut T, String> {
        match self {
            PerContext::Ready(known, _) | PerContext::Failed(known, _) if *known == context => {}
            _ => {
                // The old value goes before the new one is built, as it belongs to another context.
                *self = PerContext::Empty;
                *self = match load() {
                    Ok(value) => PerContext::Ready(context, value),
                    Err(reason) => PerContext::Failed(context, reason),
                };
            }
        }
        match self {
            PerContext::Ready(_, value) => Ok(value),
            PerContext::Failed(_, reason) => Err(format!(
                "the GPU kernel did not load in this context: {reason}"
            )),
            PerContext::Empty => Err("no kernel".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::PerContext;

    /// Counts the values dropped, to see an old context's value go.
    struct Tracked(Rc<Cell<usize>>);

    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn a_kernel_that_failed_is_not_loaded_again_in_the_same_context() {
        let mut slot: PerContext<u8, ()> = PerContext::default();
        let loads = Cell::new(0);
        let load = || {
            loads.set(loads.get() + 1);
            Err::<(), _>("CUDA_ERROR_NO_BINARY_FOR_GPU".to_string())
        };
        for _ in 0..3 {
            let err = slot.get_or_load(1, load).unwrap_err();
            assert!(err.contains("CUDA_ERROR_NO_BINARY_FOR_GPU"), "{err}");
        }
        assert_eq!(loads.get(), 1);
    }

    #[test]
    fn another_context_gets_a_new_try_and_keeps_what_loads() {
        let mut slot: PerContext<u8, u32> = PerContext::default();
        let loads = Cell::new(0);
        let fail = || {
            loads.set(loads.get() + 1);
            Err("failed".to_string())
        };
        let succeed = || {
            loads.set(loads.get() + 1);
            Ok(7)
        };
        assert!(slot.get_or_load(1, fail).is_err());
        assert_eq!(slot.get_or_load(2, succeed), Ok(&mut 7));
        assert_eq!(slot.get_or_load(2, succeed), Ok(&mut 7));
        assert_eq!(loads.get(), 2);
    }

    #[test]
    fn a_new_context_drops_the_old_value_before_loading() {
        let dropped = Rc::new(Cell::new(0));
        let mut slot: PerContext<u8, Tracked> = PerContext::default();
        slot.get_or_load(1, || Ok(Tracked(dropped.clone())))
            .unwrap();
        slot.get_or_load(1, || Ok(Tracked(dropped.clone())))
            .unwrap();
        assert_eq!(dropped.get(), 0, "the same context keeps its value");
        let dropped_before_load = Cell::new(None);
        slot.get_or_load(2, || {
            dropped_before_load.set(Some(dropped.get()));
            Ok(Tracked(dropped.clone()))
        })
        .unwrap();
        assert_eq!(dropped_before_load.get(), Some(1));
    }
}
