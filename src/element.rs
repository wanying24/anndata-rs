mod raw;

use raw::RawMatrixElem;
use crate::anndata_trait::*;
use polars::frame::DataFrame;

use hdf5::{Result, Group}; 
use std::sync::Arc;

#[derive(Clone)]
pub struct MatrixElem(pub Arc<RawMatrixElem<dyn DataPartialIO>>);

impl MatrixElem {
    pub fn new(container: DataContainer) -> Result<Self> {
        let elem = RawMatrixElem::new(container)?;
        Ok(Self(Arc::new(elem)))
    }

    pub fn write(&self, location: &Group, name: &str) -> Result<()> {
        self.0.write_elem(location, name)
    }

    pub fn nrows(&self) -> usize { self.0.nrows }

    pub fn ncols(&self) -> usize { self.0.ncols }

    pub fn subset_rows(&mut self, idx: &[usize]) {
        Arc::get_mut(&mut self.0).unwrap().subset_rows(idx);
    }

    pub fn subset_cols(&mut self, idx: &[usize]) {
        Arc::get_mut(&mut self.0).unwrap().subset_cols(idx);
    }

    pub fn subset(&mut self, ridx: &[usize], cidx: &[usize]) {
        Arc::get_mut(&mut self.0).unwrap().subset(ridx, cidx);
    }
}

#[derive(Clone)]
pub struct DataFrameElem(pub Arc<RawMatrixElem<DataFrame>>);

impl DataFrameElem {
    pub fn new(container: DataContainer) -> Result<Self> {
        let elem = RawMatrixElem::new_elem(container)?;
        Ok(Self(Arc::new(elem)))
    }

    pub fn write(&self, location: &Group, name: &str) -> Result<()> {
        self.0.write_elem(location, name)
    }

    pub fn nrows(&self) -> usize { self.0.nrows }

    pub fn ncols(&self) -> usize { self.0.ncols }

    pub fn subset_rows(&mut self, idx: &[usize]) {
        Arc::get_mut(&mut self.0).unwrap().subset_rows(idx);
    }

    pub fn subset_cols(&mut self, idx: &[usize]) {
        Arc::get_mut(&mut self.0).unwrap().subset_cols(idx);
    }

    pub fn subset(&mut self, ridx: &[usize], cidx: &[usize]) {
        Arc::get_mut(&mut self.0).unwrap().subset(ridx, cidx);
    }
}