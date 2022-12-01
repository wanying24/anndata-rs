use crate::data::*;

use anyhow::Result;
use polars::prelude::DataFrame;

pub trait AnnDataOp {
    /// Reading/writing the 'X' element.
    fn read_x<D>(&self) -> Result<Option<D>>
    where
        D: ReadData + Into<ArrayData> + TryFrom<ArrayData> + Clone,
        <D as TryFrom<ArrayData>>::Error: Into<anyhow::Error>;

    fn read_x_slice<D, S>(&self, select: S) -> Result<Option<D>>
    where
        D: ReadArrayData + Into<ArrayData> + TryFrom<ArrayData> + Clone,
        S: AsRef<[SelectInfoElem]>,
        <D as TryFrom<ArrayData>>::Error: Into<anyhow::Error>;

    fn set_x<D: WriteData + Into<ArrayData> + HasShape>(&self, data_: D) -> Result<()>;
    fn del_x(&self) -> Result<()>;

    /// Return the number of observations (rows).
    fn n_obs(&self) -> usize;
    /// Return the number of variables (columns).
    fn n_vars(&self) -> usize;

    /// Return the names of observations.
    fn obs_names(&self) -> Vec<String>;
    /// Return the names of variables.
    fn var_names(&self) -> Vec<String>;

    /// Chagne the names of observations.
    fn set_obs_names(&self, index: DataFrameIndex) -> Result<()>;
    /// Chagne the names of variables.
    fn set_var_names(&self, index: DataFrameIndex) -> Result<()>;

    fn obs_ix(&self, names: &[String]) -> Result<Vec<usize>>;
    fn var_ix(&self, names: &[String]) -> Result<Vec<usize>>;

    fn read_obs(&self) -> Result<DataFrame>;
    fn read_var(&self) -> Result<DataFrame>;

    /// Change the observation annotations. If `obs == None`, the `obs` will be
    /// removed.
    fn set_obs(&self, obs: Option<DataFrame>) -> Result<()>;
    /// Change the variable annotations. If `var == None`, the `var` will be
    /// removed.
    fn set_var(&self, var: Option<DataFrame>) -> Result<()>;

    fn uns_keys(&self) -> Vec<String>;
    fn obsm_keys(&self) -> Vec<String>;
    fn obsp_keys(&self) -> Vec<String>;
    fn varm_keys(&self) -> Vec<String>;
    fn varp_keys(&self) -> Vec<String>;

    fn read_uns_item(&self, key: &str) -> Result<Option<Data>>;
    fn read_obsm_item(&self, key: &str) -> Result<Option<ArrayData>>;
    fn read_obsp_item(&self, key: &str) -> Result<Option<ArrayData>>;
    fn read_varm_item(&self, key: &str) -> Result<Option<ArrayData>>;
    fn read_varp_item(&self, key: &str) -> Result<Option<ArrayData>>;

    fn add_uns_item<D: WriteData + Into<Data>>(&self, key: &str, data: D) -> Result<()>;
    fn add_obsm_item<D: WriteArrayData + HasShape + Into<ArrayData>>(
        &self,
        key: &str,
        data: D,
    ) -> Result<()>;
    fn add_obsp_item<D: WriteArrayData + HasShape + Into<ArrayData>>(
        &self,
        key: &str,
        data: D,
    ) -> Result<()>;
    fn add_varm_item<D: WriteArrayData + HasShape + Into<ArrayData>>(
        &self,
        key: &str,
        data: D,
    ) -> Result<()>;
    fn add_varp_item<D: WriteArrayData + HasShape + Into<ArrayData>>(
        &self,
        key: &str,
        data: D,
    ) -> Result<()>;
}
