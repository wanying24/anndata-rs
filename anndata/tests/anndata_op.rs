mod common;
use common::*;

use ndarray::Array2;
use proptest::prelude::*;
use anndata::*;
use anndata_hdf5::H5;
use pyanndata::PyAnnData;
use pyo3::prelude::*;

fn test_speacial_cases<F, T>(adata_gen: F)
where
    F: Fn() -> T,
    T: AnnDataOp,
{
    let adata = adata_gen();
    let arr = Array2::<i32>::zeros((0, 0));
    let arr2 = Array2::<i32>::zeros((10, 20));
    adata.set_x(&arr).unwrap();
    assert!(adata.obsm().add("test", &arr2).is_err());
}

fn test_io<F, T>(adata_gen: F)
where
    F: Fn() -> T,
    T: AnnDataOp,
{
    let arrays = proptest::collection::vec(0 as usize ..50, 2..4).prop_flat_map(|shape| array_strat(&shape));
    proptest!(ProptestConfig::with_cases(256), |(x in arrays)| {
        let adata = adata_gen();
        adata.set_x(&x).unwrap();
        prop_assert_eq!(adata.read_x::<ArrayData>().unwrap().unwrap(), x);
    });
}

fn test_index<F, T>(adata_gen: F)
where
    F: Fn() -> T,
    T: AnnDataOp,
{
    let arrays = proptest::collection::vec(0 as usize ..50, 2..4)
        .prop_flat_map(|shape| array_slice_strat(&shape));
    proptest!(ProptestConfig::with_cases(256), |((x, select) in arrays)| {
        let adata = adata_gen();
        adata.set_x(&x).unwrap();
        prop_assert_eq!(
            adata.read_x_slice::<ArrayData, _>(&select).unwrap().unwrap(),
            array_select(&x, select.as_slice())
        );

        adata.obsm().add("test", &x).unwrap();
        prop_assert_eq!(
            adata.obsm().get_slice::<ArrayData, _>("test", &select).unwrap().unwrap(),
            array_select(&x, select.as_slice())
        );
    });
}

fn test_iterator<F, T>(adata_gen: F)
where
    F: Fn() -> T,
    T: AnnDataOp,
{
    let arrays = proptest::collection::vec(20 as usize ..50, 2..3)
        .prop_flat_map(|shape| array_strat(&shape));
    proptest!(ProptestConfig::with_cases(10), |(x in arrays)| {
        let adata = adata_gen();
        adata.obsm().add_iter("test", array_chunks(&x, 7)).unwrap();
        prop_assert_eq!(adata.obsm().get::<ArrayData>("test").unwrap().unwrap(), x.clone());

        adata.obsm().add_iter("test2", adata.obsm().get_iter::<ArrayData>("test", 7).unwrap().map(|x| x.0)).unwrap();
        prop_assert_eq!(adata.obsm().get::<ArrayData>("test2").unwrap().unwrap(), x);
    });
}


////////////////////////////////////////////////////////////////////////////////
/// Test HDF5 backend
////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_speacial_cases_h5() {
    with_tmp_dir(|dir| {
        let file = dir.join("test.h5");
        let adata_gen = || AnnData::<H5>::new(&file).unwrap();
        test_speacial_cases(|| adata_gen());
    })
}

#[test]
fn test_io_h5() {
    with_tmp_dir(|dir| {
        let file = dir.join("test.h5");
        let adata_gen = || AnnData::<H5>::new(&file).unwrap();
        test_io(|| adata_gen());
    })
}

#[test]
fn test_index_h5() {
    with_tmp_dir(|dir| {
        let file = dir.join("test.h5");
        let adata_gen = || AnnData::<H5>::new(&file).unwrap();
        test_index(|| adata_gen());
    })
}

#[test]
fn test_iterator_h5() {
    with_tmp_dir(|dir| {
        let file = dir.join("test.h5");
        let adata_gen = || AnnData::<H5>::new(&file).unwrap();
        test_iterator(|| adata_gen());
    })
}

////////////////////////////////////////////////////////////////////////////////
/// Test Python backend
////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_speacial_cases_py() {
    pyo3::prepare_freethreaded_python();
    Python::with_gil(|py| {
        let adata_gen = || PyAnnData::new(py).unwrap();
        test_speacial_cases(|| adata_gen());
    })
}

#[test]
fn test_io_py() {
    pyo3::prepare_freethreaded_python();
    Python::with_gil(|py| {
        let adata_gen = || PyAnnData::new(py).unwrap();
        test_io(|| adata_gen());
    })
}

#[test]
fn test_index_py() {
    pyo3::prepare_freethreaded_python();
    Python::with_gil(|py| {
        let adata_gen = || PyAnnData::new(py).unwrap();
        test_index(|| adata_gen());
    })
}

#[test]
fn test_iterator_py() {
    pyo3::prepare_freethreaded_python();
    Python::with_gil(|py| {
        let adata_gen = || PyAnnData::new(py).unwrap();
        test_iterator(|| adata_gen());
    })
}