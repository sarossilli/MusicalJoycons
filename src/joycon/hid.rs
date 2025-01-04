use hidapi::{HidDevice, HidError};

pub trait HidApiTrait {
    fn device_list(&self) -> impl Iterator<Item = &hidapi::DeviceInfo>;
    fn open_path(&self, path: &hidapi::DeviceInfo) -> Result<Box<dyn HidDeviceTrait>, HidError>;
}

pub trait HidDeviceTrait {
    fn write(&self, data: &[u8]) -> Result<usize, HidError>;
    fn read(&mut self, data: &mut [u8]) -> Result<usize, HidError>;
}

// Implement the trait for the real HidApi
impl HidApiTrait for hidapi::HidApi {
    fn device_list(&self) -> impl Iterator<Item = &hidapi::DeviceInfo> {
        self.device_list()
    }

    fn open_path(&self, path: &hidapi::DeviceInfo) -> Result<Box<dyn HidDeviceTrait>, HidError> {
        Ok(Box::new(self.open_path(path.path())?))
    }
}

// Implement the trait for the real HidDevice
impl HidDeviceTrait for HidDevice {
    fn write(&self, data: &[u8]) -> Result<usize, HidError> {
        self.write(data)
    }

    fn read(&mut self, data: &mut [u8]) -> Result<usize, HidError> {
        self.read(data)
    }
}
