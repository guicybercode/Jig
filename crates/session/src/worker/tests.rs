use std::{
    io,
    sync::{Arc, Mutex, Weak, mpsc},
    time::{Duration, Instant},
};

use nix::errno::Errno;

use super::*;

struct RecordingWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
}

struct ErrorReader {
    error: Option<io::Error>,
}

impl Read for ErrorReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(self
            .error
            .take()
            .unwrap_or_else(|| io::Error::other("reader called more than once")))
    }
}

struct FailingWriter {
    first_write: Option<usize>,
    fail_flush: bool,
}

impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.first_write.take() {
            Some(count) => Ok(count.min(bytes.len())),
            None => Err(io::Error::from_raw_os_error(Errno::EBADF as i32)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.fail_flush {
            Err(io::Error::from_raw_os_error(Errno::EIO as i32))
        } else {
            Ok(())
        }
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes
            .lock()
            .expect("recording writer lock should be available")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn cancelled_queued_write_is_never_delivered_later() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let writer = RecordingWriter {
        bytes: Arc::clone(&written),
    };
    let (commands, receiver) = mpsc::sync_channel(3);
    let (reply, outcome) = mpsc::sync_channel(1);
    let request = Arc::new(WriteRequest::new(
        b"must-not-arrive".to_vec(),
        Instant::now() + Duration::from_secs(1),
        reply,
    ));
    let (next_reply, next_outcome) = mpsc::sync_channel(1);
    let next_request = Arc::new(WriteRequest::new(
        b"does-arrive".to_vec(),
        Instant::now() + Duration::from_secs(1),
        next_reply,
    ));
    assert!(request.cancel_if_queued());
    commands
        .send(WriterCommand::Write(request))
        .expect("write request should be queued");
    commands
        .send(WriterCommand::Write(next_request))
        .expect("subsequent write request should be queued");
    commands
        .send(WriterCommand::Close)
        .expect("close request should be queued");

    writer_loop(&Weak::new(), Box::new(writer), &receiver);

    assert_eq!(
        outcome
            .recv()
            .expect("writer should acknowledge cancellation"),
        WriterOutcome::Cancelled
    );
    assert_eq!(
        next_outcome
            .recv()
            .expect("writer should process the request after cancellation"),
        WriterOutcome::Written
    );
    assert_eq!(
        *written
            .lock()
            .expect("recording writer lock should be available"),
        b"does-arrive"
    );
}

#[test]
fn eof_eio_and_ebadf_are_classified_as_permanent_reader_termination() {
    let mut buffer = [0_u8; 8];
    let mut eof = io::Cursor::new(Vec::<u8>::new());
    assert_eq!(
        read_once(&mut eof, &mut buffer),
        ReadOutcome::Terminated(ReadTermination::Eof)
    );

    let mut eio = ErrorReader {
        error: Some(io::Error::from_raw_os_error(Errno::EIO as i32)),
    };
    assert_eq!(
        read_once(&mut eio, &mut buffer),
        ReadOutcome::Terminated(ReadTermination::Eio)
    );

    let mut ebadf = ErrorReader {
        error: Some(io::Error::from_raw_os_error(Errno::EBADF as i32)),
    };
    assert_eq!(
        read_once(&mut ebadf, &mut buffer),
        ReadOutcome::Terminated(ReadTermination::Error)
    );
}

#[test]
fn writer_failure_reports_whether_delivery_could_have_started() {
    let mut failed_before_delivery = FailingWriter {
        first_write: None,
        fail_flush: false,
    };
    assert_eq!(
        write_bytes(&Weak::new(), &mut failed_before_delivery, b"payload"),
        WriterOutcome::FailedBeforeDelivery
    );

    let mut partial = FailingWriter {
        first_write: Some(1),
        fail_flush: false,
    };
    assert_eq!(
        write_bytes(&Weak::new(), &mut partial, b"payload"),
        WriterOutcome::DeliveryAmbiguous
    );
}

#[test]
fn flush_failure_is_delivery_ambiguous() {
    let mut writer = FailingWriter {
        first_write: Some(usize::MAX),
        fail_flush: true,
    };

    assert_eq!(
        write_bytes(&Weak::new(), &mut writer, b"payload"),
        WriterOutcome::DeliveryAmbiguous
    );
}

#[test]
fn writer_worker_notifies_the_runtime_on_permanent_failure() {
    let (commands, receiver) = mpsc::sync_channel(1);
    let (reply, outcome) = mpsc::sync_channel(1);
    commands
        .send(WriterCommand::Write(Arc::new(WriteRequest::new(
            b"payload".to_vec(),
            Instant::now() + Duration::from_secs(1),
            reply,
        ))))
        .expect("failing write should be queued");
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_by_handler = Arc::clone(&observed);

    writer_loop_with_failure_handler(
        &Weak::new(),
        Box::new(FailingWriter {
            first_write: None,
            fail_flush: false,
        }),
        &receiver,
        move |failure| {
            observed_by_handler
                .lock()
                .expect("failure observation should remain available")
                .push(failure);
        },
    );

    assert_eq!(
        outcome.recv().expect("write outcome should be reported"),
        WriterOutcome::FailedBeforeDelivery
    );
    assert_eq!(
        *observed
            .lock()
            .expect("failure observation should remain available"),
        [WriterOutcome::FailedBeforeDelivery]
    );
}
