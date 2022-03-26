mod utils;
use utils::df_to_py;

use pyo3::prelude::*;
use numpy::{PyReadonlyArrayDyn, IntoPyArray};
use pyo3::{pymodule, types::PyModule, PyResult, Python};
use nalgebra_sparse::csr::CsrMatrix;
use hdf5::types::TypeDescriptor::*;
use hdf5::types::IntSize;
use hdf5::types::FloatSize;
use std::collections::HashMap;
use ndarray::ArrayD;
use polars::frame::DataFrame;

use anndata_rs::{
    base::AnnData,
    element::MatrixElem,
    anndata_trait::{DataType, DataSubset2D},
};

#[pyclass]
#[repr(transparent)]
pub struct PyAnnData(AnnData);

#[pymethods]
impl PyAnnData {
    fn set_x(&mut self, data: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            self.0.set_x(&py_to_data2(py, data)?).unwrap();
            Ok(())
        })
    }

    fn get_x(&self) -> PyResult<PyElem2dView> {
        Ok(PyElem2dView(self.0.x.clone()))
    }

    fn get_obs(&self) -> PyResult<PyElem2dView> {
        Ok(PyElem2dView(self.0.obs.clone()))
    }

    fn get_obsm(&self) -> PyResult<HashMap<String, PyElem2dView>> {
        let obsm = self.0.obsm.iter()
            .map(|(k, x)| (k.clone(), PyElem2dView(x.clone())))
            .collect();
        Ok(obsm)
    }

    fn get_var(&self) -> PyResult<PyElem2dView> {
        Ok(PyElem2dView(self.0.var.clone()))
    }

    fn get_varm(&self) -> PyResult<HashMap<String, PyElem2dView>> {
        let varm = self.0.varm.iter()
            .map(|(k, x)| (k.clone(), PyElem2dView(x.clone())))
            .collect();
        Ok(varm)
    }

    fn subset_rows(&self, idx: Vec<usize>) -> PyResult<PyAnnData> {
        Ok(PyAnnData(self.0.subset_obs(idx.as_slice())))
    }

    fn write(&self, filename: &str) -> PyResult<()> {
        self.0.write(filename).unwrap();
        Ok(())
    }
}

#[pyclass]
#[repr(transparent)]
pub struct PyElem2dView(MatrixElem);

#[pymethods]
impl PyElem2dView {
    fn get_data(&self) -> PyResult<Py<PyAny>> {
        Python::with_gil(|py| data_to_py(py, self.0.0.read_elem().unwrap()))
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
    data: Box<dyn DataSubset2D>,
) -> PyResult<PyObject>
{
    match data.as_ref().get_dtype() {
        DataType::CsrMatrix(Unsigned(IntSize::U4)) =>
            csr_to_scipy::<u32>(py, *data.into_any().downcast().unwrap()),
        DataType::CsrMatrix(Unsigned(IntSize::U8)) =>
            csr_to_scipy::<u64>(py, *data.into_any().downcast().unwrap()),
        DataType::CsrMatrix(Float(FloatSize::U4)) =>
            csr_to_scipy::<f32>(py, *data.into_any().downcast().unwrap()),
        DataType::CsrMatrix(Float(FloatSize::U8)) =>
            csr_to_scipy::<f64>(py, *data.into_any().downcast().unwrap()),

        DataType::Array(Float(FloatSize::U4)) => Ok((
            &*data.into_any().downcast::<ArrayD<f32>>().unwrap().into_pyarray(py)
        ).to_object(py)),
        DataType::Array(Float(FloatSize::U8)) => Ok((
            &*data.into_any().downcast::<ArrayD<f64>>().unwrap().into_pyarray(py)
        ).to_object(py)),

        DataType::DataFrame =>
            df_to_py(*data.into_any().downcast::<DataFrame>().unwrap()),

        _ => todo!(),
    }
}

fn py_to_data<'py>(
    py: Python<'py>,
    obj: PyObject,
) -> PyResult<CsrMatrix<f64>>
{
    let shape: Vec<usize> = obj.getattr(py, "shape")?.extract(py)?;
    let data = obj.getattr(py, "data")?
        .extract::<PyReadonlyArrayDyn<f64>>(py)?.to_vec().unwrap();
    let indices = obj.getattr(py, "indices")?
        .extract::<PyReadonlyArrayDyn<i32>>(py)?.as_array().iter()
        .map(|x| (*x).try_into().unwrap()).collect();
    let indptr = obj.getattr(py, "indptr")?
        .extract::<PyReadonlyArrayDyn<i32>>(py)?.as_array().iter()
        .map(|x| (*x).try_into().unwrap()).collect();
    Ok(CsrMatrix::try_from_csr_data(shape[0], shape[1], indptr, indices, data).unwrap())
}

fn py_to_data2<'py>(
    py: Python<'py>,
    obj: PyObject,
) -> PyResult<ArrayD<f64>>
{
    Ok(obj.extract::<PyReadonlyArrayDyn<f64>>(py)?.to_owned_array())
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