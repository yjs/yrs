use pyo3::prelude::*;
use pyo3::types::PyAny;
use pyo3::wrap_pyfunction;
use pythonize::{depythonize, pythonize};
use yrs;

#[pyfunction]
pub fn merge_updates(updates: Vec<Vec<u8>>) -> PyResult<Py<PyAny>> {
    // Converts a Vec<Vec<u8>>  into a   [&[u8]]
    println!("updates {:?}", updates);
    let updates_u8: Vec<&[u8]> = updates.iter().map(|x| &x[..]).collect();
    println!("updates {:?}", updates_u8);

    let result = yrs::merge_updates(&updates_u8);
    println!("result {:?}", result);

    let gil = Python::acquire_gil();
    let py = gil.python();
    Ok(pythonize(py, &result)?)
}

#[pyfunction]
pub fn encode_state_vector_from_update(update: Vec<u8>) -> PyResult<Py<PyAny>> {
    let result = yrs::encode_state_vector_from_update(&update);

    let gil = Python::acquire_gil();
    let py = gil.python();
    Ok(pythonize(py, &result)?)
}

#[pyfunction]
pub fn diff_updates(update: Vec<u8>, state_vector: Vec<u8>) -> PyResult<Py<PyAny>> {
    let result = yrs::diff_updates(&update, &state_vector);

    let gil = Python::acquire_gil();
    let py = gil.python();
    Ok(pythonize(py, &result)?)
}

#[pymodule(y_py)]
fn y_py(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(merge_updates, m)?)?;
    m.add_function(wrap_pyfunction!(encode_state_vector_from_update, m)?)?;
    m.add_function(wrap_pyfunction!(diff_updates, m)?)?;

    Ok(())
}
