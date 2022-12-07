use crate::{
    backend::{Backend, BackendData, DataContainer, DataType, GroupOp, LocationOp},
    utils::array::concat_array_data,
    //iterator::{ChunkedMatrix, StackedChunkedMatrix},
    data::*,
};

use anyhow::{bail, ensure, Result};
use indexmap::set::IndexSet;
use itertools::Itertools;
use ndarray::Ix1;
use parking_lot::{Mutex, MutexGuard};
use polars::{
    frame::DataFrame,
    prelude::{concat, IntoLazy},
    series::Series,
};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use smallvec::SmallVec;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

/// Slot stores an optional object wrapped by Arc and Mutex.
/// Encapsulating an object inside a slot allows us to drop the object from all references.
pub struct Slot<T>(pub(crate) Arc<Mutex<Option<T>>>);

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Slot(self.0.clone())
    }
}

impl<T> std::fmt::Display for Slot<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "Empty or closed slot")
        } else {
            write!(f, "{}", self.inner().deref())
        }
    }
}

impl<T> Slot<T> {
    /// Create a slot from data.
    pub fn new(x: T) -> Self {
        Slot(Arc::new(Mutex::new(Some(x))))
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Slot(Arc::new(Mutex::new(None)))
    }

    pub fn is_empty(&self) -> bool {
        self.0.lock().is_none()
    }

    pub fn lock(&self) -> MutexGuard<'_, Option<T>> {
        self.0.lock()
    }

    pub fn inner(&self) -> Inner<'_, T> {
        Inner(self.0.lock())
    }

    /// Insert data to the slot, and return the old data.
    pub fn insert(&self, data: T) -> Option<T> {
        std::mem::replace(self.0.lock().deref_mut(), Some(data))
    }

    /// Extract the data from the slot. The slot becomes empty after this operation.
    pub fn extract(&self) -> Option<T> {
        std::mem::replace(self.0.lock().deref_mut(), None)
    }

    /// Remove the data from the slot.
    pub fn drop(&self) {
        let _ = self.extract();
    }

    pub fn swap(&self, other: &Self) {
        let mut self_lock = self.0.lock();
        let mut other_lock = other.0.lock();
        std::mem::swap(self_lock.deref_mut(), other_lock.deref_mut());
    }
}

pub struct Inner<'a, T>(pub MutexGuard<'a, Option<T>>);

impl<T> Deref for Inner<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.0.deref() {
            None => panic!("accessing an empty slot"),
            Some(x) => x,
        }
    }
}

impl<T> DerefMut for Inner<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self.0.deref_mut() {
            None => panic!("accessing an empty slot"),
            Some(ref mut x) => x,
        }
    }
}

pub struct InnerDataFrameElem<B: Backend> {
    element: Option<DataFrame>,
    container: DataContainer<B>,
    column_names: IndexSet<String>,
    pub index: DataFrameIndex,
}

impl<B: Backend> InnerDataFrameElem<B> {
    pub fn new<G: GroupOp<Backend = B>>(
        location: &G,
        name: &str,
        index: DataFrameIndex,
        df: &DataFrame,
    ) -> Result<Self> {
        ensure!(
            df.height() == 0 || index.len() == df.height(),
            "cannot create dataframe element as lengths of index and dataframe differ"
        );
        df.write(location, name)?;
        let container = index.write(location, name)?;
        let column_names = df.get_column_names_owned().into_iter().collect();
        Ok(Self {
            element: None,
            container,
            column_names,
            index,
        })
    }
}

impl<B: Backend> std::fmt::Display for InnerDataFrameElem<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dataframe element")
    }
}

impl<B: Backend> InnerDataFrameElem<B> {
    pub fn height(&self) -> usize {
        self.index.len()
    }

    pub fn column(&mut self, name: &str) -> Result<&Series> {
        self.data().and_then(|x| Ok(x.column(name)?))
    }

    pub fn get_column_names(&self) -> &IndexSet<String> {
        &self.column_names
    }

    pub fn set_index(&mut self, index: DataFrameIndex) -> Result<()> {
        ensure!(
            self.index.len() == index.len(),
            "cannot change the index as the lengths differ"
        );
        self.index = index;
        replace_with::replace_with_or_abort(&mut self.container, |x| {
            self.index.overwrite(x).unwrap()
        });
        Ok(())
    }

    pub fn data(&mut self) -> Result<&DataFrame> {
        match self.element {
            Some(ref df) => Ok(df),
            None => {
                let df = DataFrame::read(&self.container)?;
                self.element = Some(df);
                Ok(&self.element.as_ref().unwrap())
            }
        }
    }

    pub fn export(&self, location: &B::Group, name: &str) -> Result<()> {
        let df = match self.element {
            Some(ref df) => df.clone(),
            None => DataFrame::read(&self.container)?,
        };
        df.write(location, name)?;
        self.index.write(location, name)?;
        Ok(())
    }

    pub fn save(&mut self, data: DataFrame) -> Result<()> {
        let num_recs = data.height();
        ensure!(
            num_recs == 0 || self.index.len() == num_recs,
            "cannot update dataframe as lengths differ"
        );
        replace_with::replace_with_or_abort(&mut self.container, |x| data.overwrite(x).unwrap());
        self.column_names = data.get_column_names_owned().into_iter().collect();
        if self.element.is_some() {
            self.element = Some(data);
        }
        Ok(())
    }

    pub fn subset_rows<S: AsRef<SelectInfoElem>>(&mut self, selection: S) -> Result<()> {
        todo!()
    }
}

pub type DataFrameElem<B> = Slot<InnerDataFrameElem<B>>;

impl<B: Backend> TryFrom<DataContainer<B>> for DataFrameElem<B> {
    type Error = anyhow::Error;

    fn try_from(container: DataContainer<B>) -> Result<Self> {
        match container.encoding_type()? {
            DataType::DataFrame => {
                //let grp = container.as_group()?;
                let index = DataFrameIndex::read(&container)?;
                let column_names = container
                    .read_arr_attr::<String, Ix1>("column_order")?
                    .into_raw_vec()
                    .into_iter()
                    .collect();
                let df = InnerDataFrameElem {
                    element: None,
                    container,
                    column_names,
                    index,
                };
                Ok(Slot::new(df))
            }
            ty => bail!("Expecting a dataframe but found: '{}'", ty),
        }
    }
}

/*

    pub fn subset_rows(&self, idx: &[usize]) -> Result<()> {
        let subset = self
            .with_data_ref(|x| {
                x.map(|(index, df)| {
                    let new_df = df.take_iter(idx.into_iter().map(|x| *x))?;
                    let new_index: DataFrameIndex =
                        idx.into_iter().map(|i| index.names[*i].clone()).collect();
                    Ok::<_, anyhow::Error>((new_index, new_df))
                })
            })?
            .transpose()?;

        if let Some((index, df)) = subset {
            let mut inner = self.inner();
            inner.container = df.update(&inner.container)?;
            index.write(&inner.container)?;
            inner.column_names = df.get_column_names_owned().into_iter().collect();
            if inner.element.is_some() {
                inner.element = Some(df);
            }
            inner.index = index;
        }
        Ok(())
    }
}
*/

/// Container holding general data types.
pub struct InnerElem<B: Backend, T> {
    dtype: DataType,
    shape: Option<Shape>,
    cache_enabled: bool,
    container: DataContainer<B>,
    element: Option<T>,
}

impl<B: Backend, T> std::fmt::Display for InnerElem<B, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} element, cache_enabled: {}, cached: {}",
            self.dtype,
            if self.cache_enabled { "yes" } else { "no" },
            if self.element.is_some() { "yes" } else { "no" },
        )
    }
}

impl<B: Backend, T> InnerElem<B, T> {
    pub fn dtype(&self) -> DataType {
        self.dtype
    }

    pub fn enable_cache(&mut self) {
        self.cache_enabled = true;
    }

    pub fn disable_cache(&mut self) {
        if self.element.is_some() {
            self.element = None;
        }
        self.cache_enabled = false;
    }
}

impl<B: Backend, T> InnerElem<B, T> {
    pub(crate) fn save<D: WriteData + Into<T>>(&mut self, data: D) -> Result<()> {
        replace_with::replace_with_or_abort(&mut self.container, |x| data.overwrite(x).unwrap());
        if self.element.is_some() {
            self.element = Some(data.into());
        }
        Ok(())
    }
}

impl<B: Backend, T: Clone> InnerElem<B, T> {
    pub fn data<D>(&mut self) -> Result<D>
    where
        D: Into<T> + ReadData + Clone + TryFrom<T>,
        <D as TryFrom<T>>::Error: Into<anyhow::Error>,
    {
        match self.element.as_ref() {
            Some(data) => Ok(data.clone().try_into().map_err(Into::into)?),
            None => {
                let data = D::read(&self.container)?;
                if self.cache_enabled {
                    self.element = Some(data.clone().into());
                }
                Ok(data)
            }
        }
    }
}

impl<B: Backend, T: ReadData + WriteData + Clone> InnerElem<B, T> {
    pub fn export(&mut self, location: &B::Group, name: &str) -> Result<()> {
        match self.element.as_ref() {
            Some(data) => data.write(location, name)?,
            None => T::read(&self.container)?.write(location, name)?,
        };
        Ok(())
    }
}

impl<B: Backend, T: ArrayOp + Clone> InnerElem<B, T> {
    pub fn shape(&self) -> &Shape {
        self.shape.as_ref().unwrap()
    }

    pub fn select<D, S, E>(&mut self, selection: S) -> Result<D>
    where
        D: Into<T> + TryFrom<T> + ReadArrayData + Clone,
        S: AsRef<[E]>,
        E: AsRef<SelectInfoElem>,
        <D as TryFrom<T>>::Error: Into<anyhow::Error>,
    {
        if selection.as_ref().iter().all(|x| x.as_ref().is_full()) {
            self.data()
        } else {
            match self.element.as_ref() {
                Some(data) => Ok(data.select(selection).try_into().map_err(Into::into)?),
                None => D::read_select(&self.container, selection),
            }
        }
    }
}

impl<B: Backend, T: ReadArrayData + WriteArrayData + ArrayOp + Clone> InnerElem<B, T> {
    pub fn export_select<S, E>(
        &mut self,
        selection: S,
        location: &B::Group,
        name: &str,
    ) -> Result<()>
    where
        S: AsRef<[E]>,
        E: AsRef<SelectInfoElem>,
    {
        if selection.as_ref().iter().all(|x| x.as_ref().is_full()) {
            self.export(location, name)
        } else {
            self.select::<T, _, _>(selection)?.write(location, name)?;
            Ok(())
        }
    }

    pub(crate) fn subset<S, E>(&mut self, selection: S) -> Result<()>
    where
        S: AsRef<[E]>,
        E: AsRef<SelectInfoElem>,
    {
        let data = match self.element.as_ref() {
            Some(data) => data.select(selection),
            None => T::read_select(&self.container, selection)?,
        };

        self.shape = Some(data.shape());
        replace_with::replace_with_or_abort(&mut self.container, |x| data.overwrite(x).unwrap());
        if self.element.is_some() {
            self.element = Some(data);
        }
        Ok(())
    }
}

pub type Elem<B> = Slot<InnerElem<B, Data>>;

impl<B: Backend> TryFrom<DataContainer<B>> for Elem<B> {
    type Error = anyhow::Error;

    fn try_from(container: DataContainer<B>) -> Result<Self> {
        let dtype = container.encoding_type()?;
        let elem = InnerElem {
            dtype,
            shape: None,
            cache_enabled: false,
            element: None,
            container,
        };
        Ok(Slot::new(elem))
    }
}

pub type ArrayElem<B> = Slot<InnerElem<B, ArrayData>>;

/// Container holding matrix data types.
impl<B: Backend> TryFrom<DataContainer<B>> for ArrayElem<B> {
    type Error = anyhow::Error;

    fn try_from(container: DataContainer<B>) -> Result<Self> {
        let dtype = container.encoding_type()?;
        let elem = InnerElem {
            dtype,
            shape: Some(ArrayData::get_shape(&container)?),
            cache_enabled: false,
            element: None,
            container,
        };
        Ok(Slot::new(elem))
    }
}

/*
pub fn chunked(&self, chunk_size: usize) -> ChunkedMatrix {
    ChunkedMatrix::new(self.clone(), chunk_size)
}
*/

/// Horizontal concatenated dataframe elements.
#[derive(Clone)]
pub struct StackedDataFrame<B: Backend> {
    column_names: IndexSet<String>,
    elems: Arc<Vec<DataFrameElem<B>>>,
}

impl<B: Backend> std::fmt::Display for StackedDataFrame<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "stacked dataframe with columns: '{}'",
            self.column_names.iter().join("', '")
        )
    }
}

impl<B: Backend> StackedDataFrame<B> {
    pub fn new(elems: Vec<DataFrameElem<B>>) -> Result<Self> {
        if elems.iter().all(|x| x.is_empty()) {
            Ok(Self {
                column_names: IndexSet::new(),
                elems: Arc::new(elems),
            })
        } else if elems.iter().all(|x| !x.is_empty()) {
            let column_names = elems
                .iter()
                .map(|x| x.inner().get_column_names().clone())
                .reduce(|shared_keys, next_keys| {
                    shared_keys
                        .intersection(&next_keys)
                        .map(|x| x.to_owned())
                        .collect()
                })
                .unwrap_or(IndexSet::new());
            Ok(Self {
                column_names,
                elems: Arc::new(elems),
            })
        } else {
            bail!("slots must be either all empty or all full");
        }
    }

    pub fn get_column_names(&self) -> &IndexSet<String> {
        &self.column_names
    }

    pub fn data(&self) -> Result<DataFrame> {
        let mut merged = DataFrame::empty();
        self.elems.iter().try_for_each(|el| {
            if let Some(el) = el.lock().as_mut() {
                merged.vstack_mut(el.data()?)?;
            }
            Ok::<(), anyhow::Error>(())
        })?;
        merged.rechunk();
        Ok(merged)
    }

    pub fn par_data(&self) -> Result<DataFrame> {
        let dfs = self
            .elems
            .par_iter()
            .flat_map(|el| {
                el.lock()
                    .as_mut()
                    .map(|el| el.data().unwrap().clone().lazy())
            })
            .collect::<Vec<_>>();
        Ok(concat(&dfs, true, true)?.collect()?)
    }

    pub fn column(&self, name: &str) -> Result<Series> {
        if self.column_names.contains(name) {
            Ok(self.data()?.column(name)?.clone())
        } else {
            bail!("key is not present");
        }
    }
}

pub struct InnerStackedArrayElem<B: Backend> {
    shape: Shape,
    pub(crate) elems: Vec<ArrayElem<B>>,
    index: VecVecIndex,
}

impl<B: Backend> std::fmt::Display for InnerStackedArrayElem<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.elems.len() == 0 {
            write!(f, "empty stacked elements")
        } else {
            write!(
                f,
                "{} stacked elements ({}) with {}",
                self.shape,
                self.elems.len(),
                self.elems[0].inner().dtype(),
            )
        }
    }
}

impl<B: Backend> InnerStackedArrayElem<B> {
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    pub fn data<D>(&self) -> Result<D>
    where
        D: Into<ArrayData> + ReadData + Clone + TryFrom<ArrayData>,
        <D as TryFrom<ArrayData>>::Error: Into<anyhow::Error>,
    {
        let arrays: Result<SmallVec<[_; 96]>> = self
            .elems
            .iter()
            .flat_map(|x| x.lock().as_mut().map(|i| i.data::<ArrayData>()))
            .collect();
        Ok(concat_array_data(arrays?).try_into().map_err(Into::into)?)
    }

    pub fn par_data<D>(&self) -> Result<D>
    where
        D: Into<ArrayData> + ReadData + Clone + TryFrom<ArrayData>,
        <D as TryFrom<ArrayData>>::Error: Into<anyhow::Error>,
    {
        let arrays: Result<Vec<_>> = self
            .elems
            .par_iter()
            .flat_map(|x| x.lock().as_mut().map(|i| i.data::<ArrayData>()))
            .collect();
        Ok(concat_array_data(arrays?).try_into().map_err(Into::into)?)
    }

    pub fn select<D, S, E>(&mut self, selection: S) -> Result<D>
    where
        D: Into<ArrayData> + TryFrom<ArrayData> + ReadArrayData + Clone,
        S: AsRef<[E]>,
        E: AsRef<SelectInfoElem>,
        <D as TryFrom<ArrayData>>::Error: Into<anyhow::Error>,
    {
        todo!()
    }

    /// Activate the cache for all elements.
    pub fn enable_cache(&self) {
        for el in self.elems.iter() {
            if let Some(x) = el.lock().as_mut() {
                x.enable_cache();
            }
        }
    }

    /// Deactivate the cache for all elements.
    pub fn disable_cache(&self) {
        for el in self.elems.iter() {
            if let Some(x) = el.lock().as_mut() {
                x.disable_cache();
            }
        }
    }
}

#[derive(Clone)]
pub struct StackedArrayElem<B: Backend>(Arc<InnerStackedArrayElem<B>>);

impl<B: Backend> Deref for StackedArrayElem<B> {
    type Target = InnerStackedArrayElem<B>;
    fn deref(&self) -> &InnerStackedArrayElem<B> {
        &self.0
    }
}

impl<B: Backend> StackedArrayElem<B> {
    pub fn new(elems: Vec<ArrayElem<B>>) -> Result<Self> {
        ensure!(
            elems
                .iter()
                .map(|x| x.lock().as_ref().map(|x| x.dtype()))
                .all_equal(),
            "all elements must have the same dtype"
        );

        let shapes: Vec<_> = elems.iter().map(|x| x.inner().shape().clone()).collect();
        ensure!(
            shapes.iter().map(|x| &x.as_ref()[1..]).all_equal(),
            "all elements must have the same shape except for the first axis"
        );
        let index: VecVecIndex = shapes.iter().map(|x| x[0]).collect();
        let mut shape = shapes[0].clone();
        shape[0] = index.len();
        Ok(Self(Arc::new(InnerStackedArrayElem {
            shape,
            elems: elems,
            index,
        })))
    }
}



/*
pub fn read(
    &self,
    ridx_: Option<&[usize]>,
    cidx: Option<&[usize]>,
) -> Result<ArrayData> {
    match ridx_ {
        Some(ridx) => {
            let index = self.index.lock();
            let (ori_idx, rows): (Vec<_>, Vec<_>) = ridx
                .iter()
                .map(|x| index.ix(x))
                .enumerate()
                .sorted_by_key(|x| x.1 .0)
                .into_iter()
                .group_by(|x| x.1 .0)
                .into_iter()
                .map(|(key, grp)| {
                    let (ori_idx, (_, inner_idx)): (Vec<_>, (Vec<_>, Vec<_>)) = grp.unzip();
                    (
                        ori_idx,
                        self.elems[key].read(Some(inner_idx.as_slice()), cidx),
                    )
                })
                .unzip();
            Ok(rstack_with_index(
                ori_idx.into_iter().flatten().collect::<Vec<_>>().as_slice(),
                rows.into_iter().collect::<Result<_>>()?,
            )?)
        }
        None => {
            let mats: Result<Vec<_>> =
                self.elems.par_iter().map(|x| x.read(None, cidx)).collect();
            Ok(rstack(mats?)?)
        }
    }
}
*/

/// This struct is used to perform index lookup for nested Vectors (vectors of vectors).
#[derive(Clone)]
pub(crate) struct VecVecIndex(SmallVec<[usize; 96]>);

impl VecVecIndex {
    pub fn new<T>(vec_of_vec: &[Vec<T>]) -> Self {
        vec_of_vec.iter().map(|x| x.len()).collect()
    }

    /// Find the outer and inner index for a given index corresponding to the
    /// flattened view.
    ///
    /// # Example
    ///
    /// ```
    /// let vec_of_vec = vec![vec![0, 1, 2], vec![3, 4], vec![5, 6]];
    /// let flatten_view = vec![0, 1, 2, 3, 4, 5, 6];
    /// let index = VecVecIndex::new(vec_of_vec);
    /// assert_eq!(index.ix(0), (0, 0));
    /// assert_eq!(index.ix(1), (0, 1));
    /// assert_eq!(index.ix(2), (0, 2));
    /// assert_eq!(index.ix(3), (1, 0));
    /// assert_eq!(index.ix(4), (1, 1));
    /// assert_eq!(index.ix(5), (2, 0));
    /// assert_eq!(index.ix(6), (2, 1));
    /// ```
    pub fn ix(&self, i: &usize) -> (usize, usize) {
        let j = self.outer_ix(i);
        (j, i - self.0[j])
    }

    /// The inverse of ix.
    pub fn inv_ix(&self, idx: (usize, usize)) -> usize {
        self.0[idx.0] + idx.1
    }

    /// Find the outer index for a given index corresponding to the flattened view.
    pub fn outer_ix(&self, i: &usize) -> usize {
        match self.0.binary_search(i) {
            Ok(i_) => i_,
            Err(i_) => i_ - 1,
        }
    }

    pub fn ix_group_by_outer<'a, I>(
        &self,
        indices: I,
    ) -> std::collections::HashMap<usize, (Vec<usize>, Vec<usize>)>
    where
        I: Iterator<Item = &'a usize>,
    {
        indices
            .map(|x| self.ix(x))
            .enumerate()
            .sorted_by_key(|(_, (x, _))| *x)
            .into_iter()
            .group_by(|(_, (x, _))| *x)
            .into_iter()
            .map(|(outer, inner)| (outer, inner.map(|(i, (_, x))| (x, i)).unzip()))
            .collect()
    }

    /// The total number of elements
    pub fn len(&self) -> usize {
        *self.0.last().unwrap_or(&0)
    }
}

impl FromIterator<usize> for VecVecIndex {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = usize>,
    {
        let index: SmallVec<_> = std::iter::once(0)
            .chain(iter.into_iter().scan(0, |state, x| {
                *state = *state + x;
                Some(*state)
            }))
            .collect();
        VecVecIndex(index)
    }
}

/*
   pub fn chunked(&self, chunk_size: usize) -> StackedChunkedMatrix {
        StackedChunkedMatrix::new(self.elems.iter().map(|x| x.clone()), chunk_size)
    }



*/