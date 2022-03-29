mod utils;
use utils::{to_py_df, to_rust_df, py_to_rust::{to_rust_data1, to_rust_data2}};

use pyo3::{
    prelude::*,
    pymodule, types::PyModule, PyResult, Python,
};
use numpy::{PyReadonlyArrayDyn, IntoPyArray};
use nalgebra_sparse::csr::CsrMatrix;
use hdf5::types::TypeDescriptor::*;
use hdf5::types::IntSize;
use hdf5::types::FloatSize;
use std::collections::HashMap;
use ndarray::ArrayD;
use polars::frame::DataFrame;

use anndata_rs::{
    base::AnnData,
    element::{Elem, MatrixElem, DataFrameElem},
    anndata_trait::{DataType, WritePartialData},
};

#[pyclass]
#[repr(transparent)]
#[derive(Clone)]
pub struct PyAnnData(AnnData);

#[pymethods]
impl PyAnnData {
    #[new]
    fn new(filename: &str, n_obs: usize, n_vars: usize) -> Self {
        PyAnnData(AnnData::new(filename, n_obs, n_vars).unwrap())
    }

    #[getter]
    fn n_obs(&self) -> PyResult<usize> {
        Ok(self.0.n_obs)
    }

    #[getter]
    fn n_vars(&self) -> PyResult<usize> {
        Ok(self.0.n_vars)
    }

    fn set_x(&mut self, data: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            self.0.set_x(&to_rust_data2(py, data.as_ref(py))?).unwrap();
            Ok(())
        })
    }

    fn get_x(&self) -> PyResult<Option<PyElem2dView>> {
        Ok(self.0.x.clone().map(PyElem2dView))
    }

    fn get_obs(&self) -> PyResult<Option<PyDataFrameElem>> {
        Ok(self.0.obs.clone().map(PyDataFrameElem))
    }

    fn set_obs(&mut self, df: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            self.0.set_obs(&to_rust_df(df.as_ref(py))?).unwrap();
            Ok(())
        })
    }

    fn get_obsm(&self, key: &str) -> PyResult<PyElem2dView> {
        Ok(PyElem2dView(self.0.obsm.get(key).unwrap().clone()))
    }

    fn set_obsm(&mut self, mut obsm: HashMap<String, PyObject>) -> PyResult<()> {
        Python::with_gil(|py| {
            let obsm_ = obsm.drain().map(|(k, v)| (k, to_rust_data2(py, v.as_ref(py)).unwrap())).collect();
            self.0.set_obsm(&obsm_).unwrap();
            Ok(())
        })
    }
    
    fn list_obsm(&self) -> PyResult<Vec<String>> {
        Ok(self.0.obsm.keys().map(|x| x.to_string()).collect())
    }

    fn add_obsm(&mut self, key: &str, data: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            self.0.add_obsm(key, &to_rust_data2(py, data.as_ref(py))?).unwrap();
            Ok(())
        })
    }

    fn get_var(&self) -> PyResult<Option<PyDataFrameElem>> {
        Ok(self.0.var.clone().map(PyDataFrameElem))
    }

    fn set_var(&mut self, df: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            self.0.set_var(&to_rust_df(df.as_ref(py))?).unwrap();
            Ok(())
        })
    }

    fn get_varm(&self) -> PyResult<HashMap<String, PyElem2dView>> {
        let varm = self.0.varm.iter()
            .map(|(k, x)| (k.clone(), PyElem2dView(x.clone())))
            .collect();
        Ok(varm)
    }

    fn set_varm(&mut self, mut varm: HashMap<String, PyObject>) -> PyResult<()> {
        Python::with_gil(|py| {
            let varm_ = varm.drain().map(|(k, v)| (k, to_rust_data2(py, v.as_ref(py)).unwrap())).collect();
            self.0.set_varm(&varm_).unwrap();
            Ok(())
        })
    }
    
    fn list_varm(&self) -> PyResult<Vec<String>> {
        Ok(self.0.varm.keys().map(|x| x.to_string()).collect())
    }

    fn add_varm(&mut self, key: &str, data: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            self.0.add_varm(key, &to_rust_data2(py, data.as_ref(py))?).unwrap();
            Ok(())
        })
    }

    fn get_uns(&self, key: &str) -> PyResult<PyElem> {
        Ok(PyElem(self.0.uns.get(key).unwrap().clone()))
    }

    fn set_uns(&mut self, mut uns: HashMap<String, PyObject>) -> PyResult<()> {
        Python::with_gil(|py| {
            let uns_ = uns.drain().map(|(k, v)| (k, to_rust_data1(py, v.as_ref(py)).unwrap())).collect();
            self.0.set_uns(&uns_).unwrap();
            Ok(())
        })
    }
    
    fn list_uns(&self) -> PyResult<Vec<String>> {
        Ok(self.0.uns.keys().map(|x| x.to_string()).collect())
    }

    fn add_uns(&mut self, key: &str, data: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            self.0.add_uns(key, &to_rust_data1(py, data.as_ref(py))?).unwrap();
            Ok(())
        })
    }

    fn subset_rows(&mut self, idx: Vec<usize>) -> PyResult<()> {
        self.0.subset_obs(idx.as_slice());
        Ok(())
    }

    fn write(&self, filename: &str) -> PyResult<()> {
        self.0.write(filename).unwrap();
        Ok(())
    }
}

#[pyclass]
#[repr(transparent)]
pub struct PyElem(Elem);

#[pymethods]
impl PyElem {
    fn get_data(&self) -> PyResult<Py<PyAny>> {
        Python::with_gil(|py| {
            let data = self.0.0.read_elem();
            let ty = data.as_ref().get_dtype();
            data_to_py(py, ty, data.into_any())
        })
    }
}

#[pyclass]
#[repr(transparent)]
pub struct PyElem2dView(MatrixElem);

#[pymethods]
impl PyElem2dView {
    fn get_data(&self) -> PyResult<Py<PyAny>> {
        Python::with_gil(|py| {
            let data = self.0.0.read_elem();
            let ty = data.as_ref().get_dtype();
            data_to_py(py, ty, data.into_any())
        })
    }
}

#[pyclass]
#[repr(transparent)]
pub struct PyDataFrameElem(DataFrameElem);

#[pymethods]
impl PyDataFrameElem {
    fn get_data(&self) -> PyResult<PyObject> {
        to_py_df(self.0.0.read_elem())
    }
}

#[pyfunction]
fn read_anndata(filename: &str, mode: &str) -> PyResult<PyAnnData> {
    let file = match mode {
        "r" => hdf5::File::open(filename).unwrap(),
        "r+" => hdf5::File::open_rw(filename).unwrap(),
        _ => panic!("Unkown mode"),
    };
    let anndata = AnnData::read(file).unwrap();
    Ok(PyAnnData(anndata))
}

fn data_to_py<'py>(
    py: Python<'py>,
    ty: DataType,
    data: Box<dyn std::any::Any>,
) -> PyResult<PyObject>
{
    match ty {
        DataType::CsrMatrix(Unsigned(IntSize::U4)) =>
            csr_to_scipy::<u32>(py, *data.downcast().unwrap()),
        DataType::CsrMatrix(Unsigned(IntSize::U8)) =>
            csr_to_scipy::<u64>(py, *data.downcast().unwrap()),
        DataType::CsrMatrix(Float(FloatSize::U4)) =>
            csr_to_scipy::<f32>(py, *data.downcast().unwrap()),
        DataType::CsrMatrix(Float(FloatSize::U8)) =>
            csr_to_scipy::<f64>(py, *data.downcast().unwrap()),

        DataType::Array(Unsigned(IntSize::U4)) => Ok((
            &*data.downcast::<ArrayD<u32>>().unwrap().into_pyarray(py)
        ).to_object(py)),
        DataType::Array(Unsigned(IntSize::U8)) => Ok((
            &*data.downcast::<ArrayD<u64>>().unwrap().into_pyarray(py)
        ).to_object(py)),
        DataType::Array(Integer(IntSize::U4)) => Ok((
            &*data.downcast::<ArrayD<i32>>().unwrap().into_pyarray(py)
        ).to_object(py)),
        DataType::Array(Integer(IntSize::U8)) => Ok((
            &*data.downcast::<ArrayD<i64>>().unwrap().into_pyarray(py)
        ).to_object(py)),
        DataType::Array(Float(FloatSize::U4)) => Ok((
            &*data.downcast::<ArrayD<f32>>().unwrap().into_pyarray(py)
        ).to_object(py)),
        DataType::Array(Float(FloatSize::U8)) => Ok((
            &*data.downcast::<ArrayD<f64>>().unwrap().into_pyarray(py)
        ).to_object(py)),

        DataType::DataFrame =>
            to_py_df(*data.downcast::<DataFrame>().unwrap()),

        ty => panic!("Cannot convert Rust element \"{:?}\" to Python object", ty)
    }
}

fn csr_to_scipy<'py, T>(
    py: Python<'py>,
    mat: CsrMatrix<T>
) -> PyResult<PyObject>
where T: numpy::Element
{
    let n = mat.nrows();
    let m = mat.ncols();
    let (intptr, indices, data) = mat.disassemble();

    let scipy = PyModule::import(py, "scipy.sparse")?;
    Ok(scipy.getattr("csr_matrix")?.call1((
        (data.into_pyarray(py), indices.into_pyarray(py), intptr.into_pyarray(py)),
        (n, m),
    ))?.to_object(py))
}

#[pymodule]
fn _anndata(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyAnnData>().unwrap();
    m.add_class::<PyElem2dView>().unwrap();

    m.add_function(wrap_pyfunction!(read_anndata, m)?)?;

    Ok(())
}