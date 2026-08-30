use riff_wave::WaveWriter;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{Cursor, Seek, Write};
use std::path::Path;

#[derive(Debug)]
pub struct WaveWriterError(String);

impl WaveWriterError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Error for WaveWriterError {}

impl fmt::Display for WaveWriterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn write_wave_samples_to_buffer<'a, I, B>(
    buffer: B,
    samples: I,
    sample_rate: u32,
    num_channels: u32,
    sample_width: u32,
) -> Result<(), WaveWriterError>
where
    I: Iterator<Item = &'a i16>,
    B: Seek + Write,
{
    let bits_per_sample = (sample_width * 8) as u16;
    let mut writer = WaveWriter::new(num_channels as u16, sample_rate, bits_per_sample, buffer)
        .map_err(|_| WaveWriterError::new("could not open the RIFF/WAVE stream for writing"))?;

    for sample in samples {
        writer.write_sample_i16(*sample).map_err(|_| {
            WaveWriterError::new("could not append a PCM sample to the WAVE stream")
        })?;
    }

    writer
        .sync_header()
        .map_err(|_| WaveWriterError::new("could not finalize the RIFF/WAVE chunk sizes"))
}

pub fn write_wave_samples_to_file<'a, I>(
    path: &Path,
    samples: I,
    sample_rate: u32,
    num_channels: u32,
    sample_width: u32,
) -> Result<(), WaveWriterError>
where
    I: Iterator<Item = &'a i16>,
{
    let mut encoded = Vec::new();
    write_wave_samples_to_buffer(
        Cursor::new(&mut encoded),
        samples,
        sample_rate,
        num_channels,
        sample_width,
    )?;

    let mut file = File::create(path).map_err(|source| {
        WaveWriterError::new(format!(
            "could not create wave file `{}`: {source}",
            path.display()
        ))
    })?;

    // write_all (not write) avoids silently truncating on a short write.
    file.write_all(&encoded).map_err(|source| {
        let _ = std::fs::remove_file(path);
        WaveWriterError::new(format!(
            "could not write wave bytes to `{}`: {source}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::SeekFrom;

    const STANDARD_WAVE_HEADER_LEN: u64 = 44;

    struct ThresholdFailingWriter {
        sink: Cursor<Vec<u8>>,
        max_bytes_before_failure: u64,
        fail_seeks: bool,
    }

    impl ThresholdFailingWriter {
        fn failing_immediately() -> Self {
            Self {
                sink: Cursor::new(Vec::new()),
                max_bytes_before_failure: 0,
                fail_seeks: false,
            }
        }

        fn failing_after_header() -> Self {
            Self {
                sink: Cursor::new(Vec::new()),
                max_bytes_before_failure: STANDARD_WAVE_HEADER_LEN,
                fail_seeks: false,
            }
        }

        fn failing_on_seek() -> Self {
            Self {
                sink: Cursor::new(Vec::new()),
                max_bytes_before_failure: u64::MAX,
                fail_seeks: true,
            }
        }
    }

    impl Write for ThresholdFailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.sink.get_ref().len() as u64 >= self.max_bytes_before_failure {
                return Err(std::io::Error::other("synthetic write failure"));
            }
            self.sink.write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.sink.flush()
        }
    }

    impl Seek for ThresholdFailingWriter {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            if self.fail_seeks {
                return Err(std::io::Error::other("synthetic seek failure"));
            }
            self.sink.seek(pos)
        }
    }

    fn sample_data() -> Vec<i16> {
        vec![0, 100, -100, 32767, -32768]
    }

    #[test]
    fn to_buffer_success_writes_a_valid_riff_wave_header() {
        let samples = sample_data();
        let mut bytes: Vec<u8> = Vec::new();

        let result =
            write_wave_samples_to_buffer(Cursor::new(&mut bytes), samples.iter(), 22050, 1, 2);

        assert!(result.is_ok());
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
    }

    #[test]
    fn to_buffer_errors_when_the_stream_cannot_be_opened() {
        let samples = sample_data();

        let result = write_wave_samples_to_buffer(
            ThresholdFailingWriter::failing_immediately(),
            samples.iter(),
            22050,
            1,
            2,
        );

        assert!(result.is_err());
    }

    #[test]
    fn to_buffer_errors_when_a_sample_write_fails() {
        let samples = sample_data();

        let result = write_wave_samples_to_buffer(
            ThresholdFailingWriter::failing_after_header(),
            samples.iter(),
            22050,
            1,
            2,
        );

        assert!(result.is_err());
    }

    #[test]
    fn to_buffer_errors_when_the_header_sync_fails() {
        let samples = sample_data();

        let result = write_wave_samples_to_buffer(
            ThresholdFailingWriter::failing_on_seek(),
            samples.iter(),
            22050,
            1,
            2,
        );

        assert!(result.is_err());
    }

    #[test]
    fn to_buffer_failure_messages_stay_distinguishable_across_all_three_branches() {
        let open_failure = write_wave_samples_to_buffer(
            ThresholdFailingWriter::failing_immediately(),
            sample_data().iter(),
            22050,
            1,
            2,
        )
        .unwrap_err()
        .to_string();

        let sample_failure = write_wave_samples_to_buffer(
            ThresholdFailingWriter::failing_after_header(),
            sample_data().iter(),
            22050,
            1,
            2,
        )
        .unwrap_err()
        .to_string();

        let sync_failure = write_wave_samples_to_buffer(
            ThresholdFailingWriter::failing_on_seek(),
            sample_data().iter(),
            22050,
            1,
            2,
        )
        .unwrap_err()
        .to_string();

        assert_ne!(open_failure, sample_failure);
        assert_ne!(open_failure, sync_failure);
        assert_ne!(sample_failure, sync_failure);
    }

    #[test]
    fn to_file_errors_when_the_parent_directory_does_not_exist() {
        let path = Path::new("/nonexistent-dengjen-test-dir-xyz/out.wav");
        let samples: Vec<i16> = vec![0, 1, 2];

        let result = write_wave_samples_to_file(path, samples.iter(), 22050, 1, 2);

        assert!(result.is_err());
    }

    #[cfg(target_os = "linux")]
    fn this_process_can_write_into_dev() -> bool {
        let probe = Path::new("/dev/.wave_writer_root_probe");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(probe)
        {
            Ok(_) => {
                let _ = std::fs::remove_file(probe);
                true
            }
            Err(_) => false,
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn to_file_errors_when_the_write_fails() {
        // /dev/full always accepts open() but fails every write() with ENOSPC.
        if this_process_can_write_into_dev() {
            eprintln!(
                "skipping: this process can write into /dev (likely root), so the \
                 production remove_file(\"/dev/full\") cleanup would delete a real device node"
            );
            return;
        }

        let path = Path::new("/dev/full");
        let samples = sample_data();

        let result = write_wave_samples_to_file(path, samples.iter(), 22050, 1, 2);

        assert!(result.is_err());
    }
}
