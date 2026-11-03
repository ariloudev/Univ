//! `NumPy` `.npy` arrays (format versions 1 to 3): little-endian 32- and 64-bit floats and integers,
//! in C or Fortran order, read whole into row-major `f64`.

use std::io::Read;

use super::{GridReadError, read_numbers};

/// The magic string that opens every `.npy` file.
const MAGIC: &[u8] = b"\x93NUMPY";

/// The element types read: little-endian floats and signed integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dtype {
    F32,
    F64,
    I32,
    I64,
}

impl Dtype {
    fn parse(descr: &str) -> Result<Self, GridReadError> {
        match descr {
            "<f4" => Ok(Self::F32),
            "<f8" => Ok(Self::F64),
            "<i4" => Ok(Self::I32),
            "<i8" => Ok(Self::I64),
            other => Err(npy(format!(
                "element type {other} is not a little-endian f4, f8, i4 or i8"
            ))),
        }
    }

    fn size(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::F64 | Self::I64 => 8,
        }
    }

    fn decode(self, bytes: &[u8]) -> Result<f64, GridReadError> {
        let four = || <[u8; 4]>::try_from(bytes).map_err(|_| npy("short element".into()));
        let eight = || <[u8; 8]>::try_from(bytes).map_err(|_| npy("short element".into()));
        match self {
            Self::F32 => Ok(f64::from(f32::from_le_bytes(four()?))),
            Self::F64 => Ok(f64::from_le_bytes(eight()?)),
            Self::I32 => Ok(f64::from(i32::from_le_bytes(four()?))),
            Self::I64 => {
                let i = i64::from_le_bytes(eight()?);
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "refused below unless exact: |i| < 2^53"
                )]
                let x = i as f64;
                if i.unsigned_abs() < 1 << 53 {
                    Ok(x)
                } else {
                    Err(npy(format!("integer {i} does not fit a float exactly")))
                }
            }
        }
    }
}

/// An array as its header declares it and its values in row-major (C) order, whatever order the
/// file stored them in.
#[derive(Clone, Debug, PartialEq)]
pub struct NpyArray {
    pub dtype: Dtype,
    /// The declared shape, slowest axis first as `NumPy` gives it; empty for a scalar.
    pub shape: Vec<usize>,
    /// Whether the file stored the values in Fortran (column-major) order.
    pub fortran_order: bool,
    /// The values in row-major order: `values[((i0 * n1) + i1) * n2 + i2]` for shape `[n0, n1, n2]`.
    pub values: Vec<f64>,
}

impl NpyArray {
    /// Reads one array, header and data, from the start of `reader`.
    ///
    /// # Errors
    ///
    /// Fails on a reader error, a missing magic string, an unsupported version or element type,
    /// a header that does not parse, or data shorter than the shape needs.
    pub fn read(mut reader: impl Read) -> Result<Self, GridReadError> {
        let header = read_header(&mut reader)?;
        let dtype = Dtype::parse(&dict_value(&header, "descr")?)?;
        let fortran_order = match dict_value(&header, "fortran_order")?.as_str() {
            "True" => true,
            "False" => false,
            other => return Err(npy(format!("fortran_order {other} is not True or False"))),
        };
        let shape = parse_shape(&dict_value(&header, "shape")?)?;
        let count = shape
            .iter()
            .try_fold(1_usize, |n, &d| n.checked_mul(d))
            .ok_or_else(|| npy("the shape overflows".into()))?;
        let stored = read_numbers(&mut reader, count, dtype.size(), |b| dtype.decode(b))?;
        let values = if fortran_order {
            to_row_major(&shape, &stored)
        } else {
            stored
        };
        Ok(Self {
            dtype,
            shape,
            fortran_order,
            values,
        })
    }

    /// The single value of a scalar (shape `()`) array.
    #[must_use]
    pub fn scalar(&self) -> Option<f64> {
        match (self.shape.as_slice(), self.values.as_slice()) {
            ([], [x]) => Some(*x),
            _ => None,
        }
    }
}

fn npy(reason: String) -> GridReadError {
    GridReadError::Npy(reason)
}

/// Reads the magic string, version and header, and returns the header text.
fn read_header(reader: &mut impl Read) -> Result<String, GridReadError> {
    let mut preamble = [0_u8; 8];
    reader.read_exact(&mut preamble)?;
    if &preamble[..6] != MAGIC {
        return Err(npy("the magic string \\x93NUMPY is missing".into()));
    }
    let length = match preamble[6] {
        1 => {
            let mut length = [0_u8; 2];
            reader.read_exact(&mut length)?;
            usize::from(u16::from_le_bytes(length))
        }
        2 | 3 => {
            let mut length = [0_u8; 4];
            reader.read_exact(&mut length)?;
            usize::try_from(u32::from_le_bytes(length))
                .map_err(|_| npy("header too long".into()))?
        }
        v => return Err(npy(format!("format version {v} is not 1, 2 or 3"))),
    };
    let mut header = vec![0_u8; length];
    reader.read_exact(&mut header)?;
    String::from_utf8(header).map_err(|_| npy("the header is not text".into()))
}

/// The text of one key's value in the header's Python dict literal, such as `'<f8'` → `<f8`,
/// `False` or `(128, 128)`.
fn dict_value(header: &str, key: &str) -> Result<String, GridReadError> {
    let missing = || npy(format!("the header has no {key}: {}", header.trim()));
    let quoted = format!("'{key}'");
    let after = header.find(&quoted).ok_or_else(missing)? + quoted.len();
    let rest = header[after..].trim_start();
    let rest = rest.strip_prefix(':').ok_or_else(missing)?.trim_start();
    let end = if rest.starts_with('(') {
        rest.find(')').map(|i| i + 1)
    } else {
        rest.find([',', '}'])
    }
    .ok_or_else(missing)?;
    Ok(rest[..end].trim().trim_matches(['\'', '"']).to_string())
}

/// `(128, 128, 128)`, `(5,)` or `()` as a shape.
fn parse_shape(text: &str) -> Result<Vec<usize>, GridReadError> {
    let inner = text
        .strip_prefix('(')
        .and_then(|t| t.strip_suffix(')'))
        .ok_or_else(|| npy(format!("shape {text} is not a tuple")))?;
    inner
        .split(',')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            d.parse()
                .map_err(|_| npy(format!("shape {text} holds {d}, not a size")))
        })
        .collect()
}

/// Values stored in Fortran (column-major) order, laid out again in row-major order.
fn to_row_major(shape: &[usize], stored: &[f64]) -> Vec<f64> {
    // The column-major stride of each axis: the first axis runs fastest.
    let strides: Vec<usize> = shape
        .iter()
        .scan(1, |stride, &n| {
            let s = *stride;
            *stride *= n;
            Some(s)
        })
        .collect();
    let mut index = vec![0_usize; shape.len()];
    let mut out = Vec::with_capacity(stored.len());
    for _ in 0..stored.len() {
        let at: usize = index.iter().zip(&strides).map(|(i, s)| i * s).sum();
        out.push(stored[at]);
        // Advance the row-major index: the last axis runs fastest.
        for (i, n) in index.iter_mut().zip(shape).rev() {
            *i += 1;
            if *i < *n {
                break;
            }
            *i = 0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A version 1.0 `.npy` file with this header dict and data.
    fn npy_file(dict: &str, data: &[u8]) -> Vec<u8> {
        let mut header = format!("{dict} ");
        while (10 + header.len() + 1) % 64 != 0 {
            header.push(' ');
        }
        header.push('\n');
        let mut file = MAGIC.to_vec();
        file.extend([1, 0]);
        file.extend(u16::try_from(header.len()).unwrap().to_le_bytes());
        file.extend(header.as_bytes());
        file.extend(data);
        file
    }

    #[test]
    fn a_c_order_float64_array_reads_row_major() {
        let data: Vec<u8> = (0..6).flat_map(|i| f64::from(i).to_le_bytes()).collect();
        let dict = "{'descr': '<f8', 'fortran_order': False, 'shape': (2, 3), }";
        let a = NpyArray::read(npy_file(dict, &data).as_slice()).unwrap();
        assert_eq!(a.shape, [2, 3]);
        assert_eq!(a.values, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(!a.fortran_order);
    }

    #[test]
    fn a_fortran_order_float32_array_is_laid_out_row_major() {
        // NumPy's np.asfortranarray(np.arange(6, dtype='<f4').reshape(2, 3)) stores 0 3 1 4 2 5.
        let stored = [0.0_f32, 3.0, 1.0, 4.0, 2.0, 5.0];
        let data: Vec<u8> = stored.iter().flat_map(|x| x.to_le_bytes()).collect();
        let dict = "{'descr': '<f4', 'fortran_order': True, 'shape': (2, 3), }";
        let a = NpyArray::read(npy_file(dict, &data).as_slice()).unwrap();
        assert_eq!(a.dtype, Dtype::F32);
        assert_eq!(a.values, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        let cube: Vec<u8> = (0..24_u8)
            .flat_map(|i| f32::from(i).to_le_bytes())
            .collect();
        let dict = "{'descr': '<f4', 'fortran_order': True, 'shape': (2, 3, 4), }";
        let a = NpyArray::read(npy_file(dict, &cube).as_slice()).unwrap();
        // Row-major [i][j][k] holds the stored element i + 2 j + 6 k.
        assert_eq!(a.values[(3 + 2) * 4 + 3], f64::from(1 + 2 * 2 + 6 * 3));
    }

    #[test]
    fn a_scalar_integer_reads_as_its_value() {
        let dict = "{'descr': '<i8', 'fortran_order': False, 'shape': (), }";
        let a = NpyArray::read(npy_file(dict, &9739_i64.to_le_bytes()).as_slice()).unwrap();
        assert_eq!(a.scalar(), Some(9739.0));
    }

    #[test]
    fn big_endian_unknown_and_short_arrays_are_refused() {
        let big = "{'descr': '>f8', 'fortran_order': False, 'shape': (1,), }";
        let err = NpyArray::read(npy_file(big, &[0; 8]).as_slice()).unwrap_err();
        assert!(err.to_string().contains(">f8"), "{err}");
        let short = "{'descr': '<f8', 'fortran_order': False, 'shape': (2,), }";
        assert!(NpyArray::read(npy_file(short, &[0; 8]).as_slice()).is_err());
        assert!(NpyArray::read(&b"\x93NUMPX\x01\x00"[..]).is_err());
    }
}
