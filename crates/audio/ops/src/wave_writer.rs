use riff_wave::WaveWriter;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{Cursor, Seek, Write};
use std::path::Path;

#[derive(Debug)]
pub struct WaveWriterError(String);

impl Error for WaveWriterError {}

impl fmt::Display for WaveWriterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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
    let mut writer = match WaveWriter::new(
        num_channels as u16,
        sample_rate,
        (sample_width * 8) as u16,
        buffer,
    ) {
        Ok(w) => w,
        Err(_) => {
            return Err(WaveWriterError(
                "Failed to initialize wave writer".to_string(),
            ))
        }
    };

    for sample in samples {
        if writer.write_sample_i16(*sample).is_err() {
            return Err(WaveWriterError("Failed to write wave samples".to_string()));
        }
    }

    if writer.sync_header().is_err() {
        return Err(WaveWriterError("Failed to update wave header".to_string()));
    }

    Ok(())
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
    let mut buffer: Vec<u8> = Vec::new();
    write_wave_samples_to_buffer(
        Cursor::new(&mut buffer),
        samples,
        sample_rate,
        num_channels,
        sample_width,
    )?;

    let mut file = match File::create(path) {
        Ok(f) => f,
        Err(e) => {
            return Err(WaveWriterError(format!(
                "Failed to create file `{}` for writing. Error: {}",
                path.display(),
                e
            )))
        }
    };

    if let Err(e) = file.write_all(&buffer) {
        let _ = std::fs::remove_file(path);
        return Err(WaveWriterError(format!(
            "Failed to write wave bytes to file `{}`. Error: {}",
            path.display(),
            e
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_wave_samples_to_buffer_produces_a_valid_riff_wave_header() {
        let samples: Vec<i16> = vec![0, 100, -100, 32767, -32768];
        let mut buf: Vec<u8> = Vec::new();
        let result = write_wave_samples_to_buffer(
            std::io::Cursor::new(&mut buf),
            samples.iter(),
            22050,
            1,
            2,
        );
        assert!(result.is_ok());
        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(&buf[8..12], b"WAVE");
    }

    #[test]
    fn write_wave_samples_to_file_errors_when_parent_directory_does_not_exist() {
        let path = Path::new("/nonexistent-dengjen-test-dir-xyz/out.wav");
        let samples: Vec<i16> = vec![0, 1, 2];
        let result = write_wave_samples_to_file(path, samples.iter(), 22050, 1, 2);
        assert!(result.is_err());
    }

    /// `Write` impl that returns a short write (`Ok(n) with n < buf.len()`)
    /// on its first call, then writes fully on subsequent calls. Used to
    /// prove `write_all` loops past short writes instead of treating a
    /// partial write as complete success.
    struct ShortWriteThenFull {
        data: Vec<u8>,
        first_call_done: bool,
    }

    impl Write for ShortWriteThenFull {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if !self.first_call_done {
                self.first_call_done = true;
                let n = buf.len().min(1);
                self.data.extend_from_slice(&buf[..n]);
                Ok(n)
            } else {
                self.data.extend_from_slice(buf);
                Ok(buf.len())
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_all_loops_past_a_short_write_until_the_full_buffer_lands() {
        let buffer = vec![1u8, 2, 3, 4, 5];
        let mut writer = ShortWriteThenFull {
            data: Vec::new(),
            first_call_done: false,
        };

        writer
            .write_all(&buffer)
            .expect("write_all should retry past the short write");

        assert_eq!(writer.data, buffer);
    }
}
