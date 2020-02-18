use crate::data::{ColumnType, Value};
use crate::data::{Row, Rows, Stream};
use crossbeam::{Receiver, bounded, unbounded, Sender};
use crate::errors::{CrushError, error, CrushResult, to_job_error};
use crate::replace::Replace;

pub struct ValueSender {
    sender: Sender<Value>,
}

impl ValueSender {
    pub fn send(self, cell: Value) -> CrushResult<()> {
        to_job_error(self.sender.send(cell))?;
        Ok(())
    }

    pub fn initialize(self, signature: Vec<ColumnType>) -> CrushResult<OutputStream> {
        let (output, input) = streams(signature);
        self.send(Value::Stream(Stream { stream: input }))?;
        Ok(output)
    }
}

#[derive(Debug)]
pub struct ValueReceiver {
    receiver: Receiver<Value>,
}

impl ValueReceiver {
    pub fn recv(self) -> CrushResult<Value> {
        to_job_error(self.receiver.recv())
    }
}

pub enum OutputStream {
    Sync(Sender<Row>),
    Async(Sender<Row>),
}

impl OutputStream {
    pub fn send(&self, row: Row) -> CrushResult<()> {
        let native_output = match self {
            OutputStream::Sync(s) => s.send(row),
            OutputStream::Async(s) => s.send(row),
        };
        return match native_output {
            Ok(_) => Ok(()),
            Err(_) => error("Broken pipe"),
        };
    }
}

#[derive(Debug, Clone)]
pub struct InputStream {
    receiver: Receiver<Row>,
    types: Vec<ColumnType>,
}

impl InputStream {
    pub fn recv(&self) -> CrushResult<Row> {
        self.validate(to_job_error(self.receiver.recv()))
    }

    pub fn types(&self) -> &Vec<ColumnType> {
        &self.types
    }

    fn validate(&self, res: CrushResult<Row>) -> CrushResult<Row> {
        match &res {
            Ok(row) => {
                if row.cells().len() != self.types.len() {
                    return error("Wrong number of columns in input");
                }
                for (c, ct) in row.cells().iter().zip(self.types.iter()) {
                    if c.value_type() != ct.cell_type {
                        return error(format!(
                            "Wrong cell type in input column {:?}, expected {:?}, got {:?}",
                            ct.name,
                            c.value_type(),
                            ct.cell_type).as_str());
                    }
                }
                res
            },
            Err(_) => res,
        }
    }
}

pub fn channels() -> (ValueSender, ValueReceiver) {
    let (send, recv) = bounded(1);
    (ValueSender {sender: send}, ValueReceiver { receiver: recv })
}

pub fn streams(signature: Vec<ColumnType>) -> (OutputStream, InputStream) {
    let (output, input) = bounded(128);
    (OutputStream::Sync(output), InputStream { receiver: input, types: signature })
}

pub fn unlimited_streams(signature: Vec<ColumnType>) -> (OutputStream, InputStream) {
    let (output, input) = unbounded();
    (OutputStream::Async(output), InputStream { receiver: input, types: signature })
}

pub fn empty_channel() -> ValueReceiver {
    let (o, i) = channels();
    o.send(Value::empty_stream());
    i
}

pub trait Readable {
    fn read(&mut self) -> CrushResult<Row>;
    fn types(&self) -> &Vec<ColumnType>;
}

impl Readable for InputStream {
    fn read(&mut self) -> Result<Row, CrushError> {
        match self.recv() {
            Ok(v) => Ok(v),
            Err(e) => error(&e.message),
        }
    }

    fn types(&self) -> &Vec<ColumnType> {
        self.types()
    }
}
