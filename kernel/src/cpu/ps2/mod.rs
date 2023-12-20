//! Driver for an 8042 PS/2 controller

pub mod devices;

use log::debug;
use x86_64::instructions::{hlt, port::Port};

use crate::global_state::{GlobalState, KERNEL_STATE};
use devices::Ps2Device;

#[bitfield(u8)]
struct StatusRegister {
    /// Whether there is data ready for the OS to read
    read_data_queued: bool,
    /// Whether there is data the OS has written waiting for a device to read
    write_data_queued: bool,
    /// Whether the PS/2 controller POSTed.
    /// This should always be `true` or else the OS should not be running.
    system_flag: bool,
    /// Whether a write to the data register is data for a controller
    /// command or whether it is for a PS/2 device
    write_is_for_command: bool,
    /// Reserved bits used for different things on different controllers and chipsets
    #[bits(2)]
    reserved: u8,
    /// Whether a time-out error has occurred
    time_out_error: bool,
    /// Whether a parity error has occured
    parity_error: bool,
}

#[bitfield(u8)]
struct ConfigurationRegister {
    primary_port_interrupts_enabled: bool,
    secondary_port_interrupts_enabled: bool,
    system_flag: bool,

    #[bits(1)]
    reserved0: u8,

    primary_port_clock_disabled: bool,
    secondary_port_clock_disabled: bool,
    primary_port_translation: bool,

    #[bits(1)]
    reserved1: u8,
}

/// The port number to write data to
const DATA_PORT: u16 = 0x60;
/// The port number to write commands to
const COMMAND_PORT: u16 = 0x64;

/// The number of [`ticks`][crate::global_state::KernelState::ticks]
/// which the controller will wait for data before giving up
const TIMEOUT_TRIES: usize = 5;

/// The global PS/2 controller
pub static PS2_CONTROLLER: GlobalState<Ps2Controller8042> = GlobalState::new();

/// The ports which the OS uses to drive an 8042 PS/2 controller
#[derive(Debug)]
struct Ps2Ports {
    /// The data port
    data: Port<u8>,
    /// The command / status port
    command: Port<u8>,
}

/// An 8042 PS/2 controller
#[derive(Debug)]
pub struct Ps2Controller8042 {
    /// The ports to drive the controller
    ports: Ps2Ports,
    /// Whether the controller has a second port
    dual_channelled: bool,
    /// A device connection to the primary port
    primary_port_connection: Option<Ps2Device>,
    /// A device connection to the secondary port
    secondary_port_connection: Option<Ps2Device>,
}

/// An error which can occur when a port fails its test
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2PortTestFailureError {
    /// The clock line is stuck low
    ClockLineStuckLow,
    /// The clock line is stuck high
    ClockLineStuckHigh,
    /// The data line is stuck low
    DataLineStuckLow,
    /// The data line is stuck high
    DataLineStuckHigh,

    /// The port didn't respond to the test command
    NoResponse,
    /// Another error
    Unknown,
}

impl Ps2PortTestFailureError {
    /// Parses an error from the byte received from the port
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::ClockLineStuckLow,
            1 => Self::ClockLineStuckHigh,
            2 => Self::DataLineStuckLow,
            3 => Self::DataLineStuckHigh,

            _ => Self::Unknown,
        }
    }
}

/// An error which can occur when setting up a PS/2 controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2ControllerInitialisationError {
    /// The controller's test failed
    ControllerTestFailed,
    /// A port test failed
    PortTestFailed(Ps2Port, Ps2PortTestFailureError),
    /// A port failed to re-initialise
    PortReinitError(Ps2Port),
    /// The OS sent data to a PS/2 device but the output buffer was full
    OutputBufferBlocked,
    /// The OS expected data from the controller or a device but it didn't arrive
    MissingData,
}

impl Ps2Controller8042 {
    /// Constructs a new [`Ps2Controller8042`], if the system has one
    ///
    /// # Safety
    /// This function may only be called once.
    pub unsafe fn new() -> Option<Result<Self, Ps2ControllerInitialisationError>> {
        let lock = KERNEL_STATE.acpica.lock();

        let fadt = lock.fadt();

        // If the system has no 8042 controller
        // If the table doesn't have the `has_8042_controller` field,
        // assume that it does have a controller.
        let has_8042_controller = match fadt.boot_architecture_flags() {
            Some(flags) => flags.has_8042_controller(),
            None => true,
        };

        if !has_8042_controller {
            return None;
        }

        let data = Port::new(DATA_PORT);
        let command = Port::new(COMMAND_PORT);

        let mut s = Self {
            ports: Ps2Ports { data, command },
            dual_channelled: false,
            primary_port_connection: None,
            secondary_port_connection: None,
        };

        // SAFETY: This function is only called once.
        match unsafe { s.init() } {
            Ok(_) => Some(Ok(s)),
            Err(e) => Some(Err(e)),
        }
    }

    /// Polls the device on the given `port`.
    ///
    /// # Safety
    /// This method must only be called from the interrupt handler for the given `port`.
    pub unsafe fn poll(&mut self, port: Ps2Port) {
        match port {
            Ps2Port::Primary => {
                if let Some(ref mut device) = self.primary_port_connection {
                    // SAFETY: This method is called from the interrupt handler from this device.
                    // The device exists as it was detected in `init`.
                    unsafe { device.poll(Ps2Port::Primary, &mut self.ports) }
                }
            }
            Ps2Port::Secondary => {
                if let Some(ref mut device) = self.secondary_port_connection {
                    // SAFETY: This method is called from the interrupt handler from this device.
                    // The device exists as it was detected in `init`.
                    unsafe { device.poll(Ps2Port::Secondary, &mut self.ports) }
                }
            }
        }
    }

    /// Disables the controller by sending [`DisablePrimaryPort`] and
    /// [`DisableSecondaryPort`] commands
    ///
    /// # Safety
    /// This will cause the devices to stop reporting data.
    /// The caller must ensure that either the controller is re-enabled or that no other
    /// code is relying on the data.
    /// 
    /// [`DisablePrimaryPort`]: Ps2ControllerCommand::DisablePrimaryPort
    /// [`DisableSecondaryPort`]: Ps2ControllerCommand::DisableSecondaryPort
    unsafe fn disable(&mut self) -> Result<(), Ps2ControllerInitialisationError> {
        // SAFETY: This will disable the PS/2 controller' first port
        unsafe {
            self.ports
                .send_command(Ps2ControllerCommand::DisablePrimaryPort)?
        }
        // SAFETY: This will disable the PS/2 controller' second port
        unsafe {
            self.ports
                .send_command(Ps2ControllerCommand::DisableSecondaryPort)?
        }

        Ok(())
    }

    /// Parses a sequence of bytes received from the identify command (TODO: enum-ify and link)
    /// into the device type it represents.
    const fn parse_device_id(bytes: [Option<u8>; 2]) -> Ps2Device {
        match bytes {
            [None, Some(_)] => panic!("Invalid device id bytes"),
            [None, _] => Ps2Device::ATKeyboard,
            [Some(0x00), _] => Ps2Device::StandardMouse,
            [Some(0x03), _] => Ps2Device::MouseWithScrollWheel,
            [Some(0x04), _] => Ps2Device::FiveButtonMouse,
            [Some(0xAB), Some(0x83) | Some(0xC1)] => Ps2Device::new_keyboard(),
            [Some(0xAB), Some(0x84)] => Ps2Device::ShortKeyboard,
            [_, _] => Ps2Device::Unknown,
        }
    }

    /// Flushes the read buffer, discarding any read data, and then waits for the output buffer to clear.
    unsafe fn flush_buffers(&mut self) -> Result<(), Ps2ControllerInitialisationError> {
        // SAFETY: This will flush any queued data from PS/2 devices.
        // The devices have not been initialised yet so the data is okay to be discarded.
        unsafe {
            while self.ports.read_status().read_data_queued() {
                self.ports.read_timeout();
            }
        }

        self.ports.wait_for_write_buffer_empty()
    }

    /// Initialises the PS/2 controller.
    ///
    /// # Safety
    /// This method may only be called once, and only during booting.
    unsafe fn init(&mut self) -> Result<(), Ps2ControllerInitialisationError> {
        debug!(target: "ps2_debug", "Disabling controller");

        // SAFETY: This will disable the controller while it is being set up.
        unsafe { self.disable()? }

        debug!(target: "ps2_debug", "Flushing buffers");

        // SAFETY:
        unsafe { self.flush_buffers()? }

        debug!(target: "ps2_debug", "Disabling ports");

        // Disable translation and interrupts, and check whether the controller is dual-channelled
        let has_secondary_port = {
            debug!(target: "ps2_debug", "Reading config");
            let mut config = self.ports.read_configuration()?;

            debug!(target: "ps2_debug", "Updating values");

            config.set_primary_port_translation(false);
            config.set_primary_port_interrupts_enabled(false);
            config.set_secondary_port_interrupts_enabled(false);

            debug!(target: "ps2_debug", "Writing config");

            // SAFETY: This write will disable translation and interrupts for both devices.
            unsafe { self.ports.write_configuration(config)? }

            // Check whether the controller disabled the secondary port based on
            // the `disable secondary port` command sent earlier.
            // If it did, this means the controller has two ports.
            config.secondary_port_clock_disabled()
        };

        self.dual_channelled = has_secondary_port;

        debug!(target: "ps2_debug", "Controller is dual-channelled: {has_secondary_port}");
        debug!(target: "ps2_debug", "Running tests");

        // Test components
        // SAFETY: The controller is disabled and is not in operation.
        unsafe {
            self.ports.test_controller()?;
            self.ports.test_port(Ps2Port::Primary)?;
            if has_secondary_port {
                self.ports.test_port(Ps2Port::Secondary)?;
            }
        }

        debug!(target: "ps2_debug", "Re-enabling devices");

        // Re-enable devices
        // SAFETY: The devices being enabled have interrupt handlers registered for them.
        unsafe {
            self.ports
                .send_command(Ps2ControllerCommand::EnablePrimaryPort)?;

            if let Some(mut d1) = self.ports.reinit_port(Ps2Port::Primary)? {
                d1.init(Ps2Port::Primary, &mut self.ports)?;

                debug!(target: "ps2_debug", "device connected to port 1: {:?}", d1);

                self.primary_port_connection = Some(d1);
            }

            if has_secondary_port {
                // Disable the primary port while setting up the secondary one so that data
                // from the primary port is not misinterpreted as being from the secondary port.
                self.ports
                    .send_command(Ps2ControllerCommand::DisablePrimaryPort)?;

                self.ports
                    .send_command(Ps2ControllerCommand::EnableSecondaryPort)?;

                if let Some(mut d2) = self.ports.reinit_port(Ps2Port::Secondary)? {
                    d2.init(Ps2Port::Secondary, &mut self.ports)?;

                    debug!(target: "ps2_debug", "device connected to port 1: {:?}", d2);

                    self.secondary_port_connection = Some(d2);
                }

                self.ports
                    .send_command(Ps2ControllerCommand::EnablePrimaryPort)?;
            }
        }

        debug!(target: "ps2_debug", "Enabling interrupts");

        // Re-enable interrupts for both ports
        // SAFETY: TODO
        unsafe {
            let mut config = self.ports.read_configuration()?;

            config.set_primary_port_interrupts_enabled(true);

            if has_secondary_port {
                config.set_secondary_port_interrupts_enabled(true);
            }

            self.ports.write_configuration(config)?;
        }

        Ok(())
    }
}

/// Methods to read from the PS/2 controller
impl Ps2Ports {
    /// Sends a command to the controller.
    ///
    /// # Safety
    /// The caller must ensure that the side-effects of the command do not cause UB
    /// or corrupt the controller's state.
    unsafe fn send_command(
        &mut self,
        command: Ps2ControllerCommand,
    ) -> Result<(), Ps2ControllerInitialisationError> {
        self.wait_for_write_buffer_empty()?;

        // SAFETY: The safety of this operation is the caller's responsibility
        unsafe { self.command.write(command.as_u8()) }

        Ok(())
    }

    /// Reads the controller's status register
    fn read_status(&mut self) -> StatusRegister {
        // SAFETY: Reading from the command register returns the status register
        unsafe { self.command.read().into() }
    }

    /// Reads the controller's configuration register
    fn read_configuration(
        &mut self,
    ) -> Result<ConfigurationRegister, Ps2ControllerInitialisationError> {
        self.wait_for_write_buffer_empty()?;

        debug!(target: "ps2_read_configuration", "Status is valid: sending read command");

        // SAFETY: The first byte of the controller's memory is the configuration register.
        unsafe { self.send_command(Ps2ControllerCommand::ReadByte(0))? }

        debug!(target: "ps2_read_configuration", "Reading byte");

        // SAFETY: The above command writes its result to the data port.
        unsafe {
            Ok(self
                .read_timeout()
                .ok_or(Ps2ControllerInitialisationError::MissingData)?
                .into())
        }
    }

    /// Writes the controller's configuration register
    ///
    /// # Safety
    /// Writing to the configuration register will change the PS/2 controller's behaviour.
    /// Is is the caller's responsibility that the new state is correct and compatible
    /// with other code which interacts with this controller.
    unsafe fn write_configuration(
        &mut self,
        value: ConfigurationRegister,
    ) -> Result<(), Ps2ControllerInitialisationError> {
        self.wait_for_write_buffer_empty()?;

        // SAFETY: The first byte of the controller's memory is the configuration register.
        unsafe { self.send_command(Ps2ControllerCommand::WriteByte(0))? }

        assert!(self.read_status().write_is_for_command());

        // SAFETY: The above command writes its result to the data port.
        unsafe { self.data.write(value.into()) }

        Ok(())
    }

    /// Tests a component of the controller using a given test command.
    /// The output is compared with 0x55 and a [`PortTestFailed`]
    /// is returned if the result does not match.
    /// 
    /// [`PortTestFailed`]: Ps2ControllerInitialisationError::PortTestFailed
    unsafe fn send_test_command(
        &mut self,
        command: Ps2ControllerCommand,
    ) -> Result<u8, Ps2ControllerInitialisationError> {
        // Check that the command is really a test command
        match command {
            Ps2ControllerCommand::TestController
            | Ps2ControllerCommand::TestPrimaryPort
            | Ps2ControllerCommand::TestSecondaryPort => (),
            _ => panic!("{command:?} is not a test command"),
        }

        // Some controllers reset when given a test command, so back up the configuration register
        // in order to restore it after the test
        let config = self.read_configuration()?;

        // SAFETY: The controller's state will be restored after this is complete
        unsafe { self.send_command(command)? }

        // SAFETY: The data from the test command is sent to the data register
        let response = unsafe { self.read_timeout().ok_or(command.get_timeout_error())? };

        // Write back the saved configuration
        // SAFETY: This configuration was just read and so will not change the controller's behaviour
        unsafe { self.write_configuration(config)? }

        Ok(response)
    }

    /// Sends the [`TestController`] command to test whether the controller is operational,
    /// and parses the result.
    ///
    /// # Safety
    /// This command might reset the controller depending on what model it is, so this method should not
    /// be called while the controller is in operation.
    /// 
    /// [`TestController`]: Ps2ControllerCommand::TestController
    unsafe fn test_controller(&mut self) -> Result<(), Ps2ControllerInitialisationError> {
        // SAFETY: The safety of this operation is the caller's responsibility
        unsafe {
            match self
                .send_test_command(Ps2ControllerCommand::TestController)
                .map_err(|_| Ps2ControllerInitialisationError::ControllerTestFailed)?
            {
                0x55 => Ok(()),
                _ => Err(Ps2ControllerInitialisationError::ControllerTestFailed),
            }
        }
    }

    /// Reads a byte of data from a PS/2 device. If no data is queued, `None` is returned.
    ///
    /// # Safety
    /// The caller must make sure that the data is properly parsed and responded to.
    pub unsafe fn read(&mut self) -> Option<u8> {
        // SAFETY: The safety of this operation is the caller's responsibility
        unsafe {
            if self.read_status().read_data_queued() {
                Some(self.data.read())
            } else {
                None
            }
        }
    }

    /// Reads a byte of data from a PS/2 device. If no data is given, the method will retry up to `tries` times.
    ///
    /// # Safety
    /// The caller must make sure that the data is properly parsed and responded to.
    pub unsafe fn read_timeout(&mut self) -> Option<u8> {
        let target_value = KERNEL_STATE.ticks() + TIMEOUT_TRIES;

        while KERNEL_STATE.ticks() < target_value {
            // SAFETY: The safety of this operation is the caller's responsibility
            unsafe {
                if let Some(data) = self.read() {
                    return Some(data);
                }
            }

            hlt();
        }

        None
    }
}

/// One of the two PS/2 ports on an 8042-style controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2Port {
    /// The primary port. All 8042-style controllers have this port.
    Primary,
    /// The secondary port. Not every 8042-style controller has this port.
    Secondary,
}

impl Ps2Port {
    /// Gets the command to test the port
    fn get_test_command(&self) -> Ps2ControllerCommand {
        match self {
            Ps2Port::Primary => Ps2ControllerCommand::TestPrimaryPort,
            Ps2Port::Secondary => Ps2ControllerCommand::TestSecondaryPort,
        }
    }
}

impl Ps2Ports {
    /// Sends the [`TestPrimaryPort`] command to test whether the controller is operational,
    /// and parses the result.
    ///
    /// # Safety
    /// This command might reset the controller depending on what model it is, so this method should not
    /// be called while the controller is in operation.
    /// 
    /// [`TestPrimaryPort`]: Ps2ControllerCommand::TestPrimaryPort
    unsafe fn test_port(&mut self, port: Ps2Port) -> Result<(), Ps2ControllerInitialisationError> {
        // SAFETY: The safety of this operation is the caller's responsibility
        unsafe {
            match self.send_test_command(port.get_test_command())? {
                0 => Ok(()),
                err => Err(Ps2ControllerInitialisationError::PortTestFailed(
                    port,
                    Ps2PortTestFailureError::from_u8(err),
                )),
            }
        }
    }

    /// Writes a byte to the given port.
    ///
    /// # Safety
    /// The caller must ensure that the byte written is valid and has the intended effect.
    unsafe fn write_port(
        &mut self,
        port: Ps2Port,
        value: u8,
    ) -> Result<(), Ps2ControllerInitialisationError> {
        if let Ps2Port::Secondary = port {
            // SAFETY: This means that the command will go to the secondary port.
            unsafe { self.send_command(Ps2ControllerCommand::SecondaryWrite)? }
        }

        // SAFETY: The caller is responsible for the effects of this command.
        unsafe { self.write_timeout(value) }
    }

    /// Writes `value` to the controller. This method waits up to [`TIMEOUT_TRIES`]
    /// kernel ticks for the output buffer to be free before giving up.
    ///
    /// # Safety
    /// The caller must ensure that the byte written is valid and has the intended effect.
    unsafe fn write_timeout(&mut self, value: u8) -> Result<(), Ps2ControllerInitialisationError> {
        self.wait_for_write_buffer_empty()?;

        // SAFETY: The safety of this operation is the caller's responsibility.
        unsafe { self.data.write(value) }
        Ok(())
    }

    /// Waits up to [`TIMEOUT_TRIES`]  kernel ticks for the output buffer to be free.
    fn wait_for_write_buffer_empty(&mut self) -> Result<(), Ps2ControllerInitialisationError> {
        let target_value = KERNEL_STATE.ticks() + TIMEOUT_TRIES;

        while KERNEL_STATE.ticks() < target_value {
            if self.read_status().write_data_queued() {
                hlt();
                continue;
            }

            return Ok(());
        }

        Err(Ps2ControllerInitialisationError::OutputBufferBlocked)
    }

    /// Writes a command byte to the given port and checks whether the response is ok (0xFA).
    ///
    /// # Safety
    /// The caller must ensure that the command written has the intended effect.
    unsafe fn port_send_command(
        &mut self,
        port: Ps2Port,
        command: Ps2DeviceCommand,
    ) -> Result<Option<()>, Ps2ControllerInitialisationError> {
        // SAFETY: The caller is responsible for the effect of the command
        unsafe { self.write_port(port, command.to_u8())? }

        // SAFETY: The device will send a response byte most of the time.
        match unsafe { self.read_timeout() } {
            None => Ok(None),
            Some(0xFA | 0xAA) => Ok(Some(())),
            Some(_) => Err(Ps2ControllerInitialisationError::PortReinitError(port)),
        }
    }

    /// Re-initialises the given PS/2 port, sends the identify command (TODO: enum-ify and link) and parses the response.
    ///
    /// # Safety
    /// This method should only be called during initialisation and interrupts should be disabled for both ports.
    unsafe fn reinit_port(
        &mut self,
        port: Ps2Port,
    ) -> Result<Option<Ps2Device>, Ps2ControllerInitialisationError> {
        // SAFETY: This is the reset command, which all devices should support.
        unsafe {
            match self.port_send_command(port, Ps2DeviceCommand::Reset)? {
                Some(_) => (),
                None => return Ok(None),
            }
        }

        // SAFETY: Interrupts are disabled, so these reads are guaranteed
        // not to read junk data from the other device
        unsafe {
            self.read_timeout();
            self.read_timeout();
            self.read_timeout();
            self.read_timeout();
        }

        // SAFETY: This will prevent user input from being misinterpreted as data about the device
        unsafe {
            self.port_send_command(port, Ps2DeviceCommand::DisableScanning)?
                .ok_or(Ps2ControllerInitialisationError::PortReinitError(port))?;
        }

        // SAFETY: This will cause the device to identify itself.
        // The sent bytes will all be read by this function.
        unsafe {
            self.port_send_command(port, Ps2DeviceCommand::Identify)?
                .ok_or(Ps2ControllerInitialisationError::PortReinitError(port))?;
        }

        // SAFETY: This data will be from the device identifying itself.
        let data = unsafe { [self.read_timeout(), self.read_timeout()] };

        let device_id = Ps2Controller8042::parse_device_id(data);

        // SAFETY: This command will make the device start reporting data
        unsafe {
            self.port_send_command(port, Ps2DeviceCommand::EnableScanning)?
                .ok_or(Ps2ControllerInitialisationError::PortReinitError(port))?;
        }

        Ok(Some(device_id))
    }
}

/// A command which can be send to an 8042 PS/2 controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Ps2ControllerCommand {
    /// Read the byte at a given index into the controller's memory
    /// The index must be less than 0x20.
    ReadByte(u8),
    /// Write the byte at a given index into the controller's memory
    /// The index must be less than 0x20.
    WriteByte(u8),

    /// Disable the secondary PS/2 port
    DisableSecondaryPort,
    /// Enable the secondary PS/2 port
    EnableSecondaryPort,

    /// Test the secondary PS/2 port
    TestSecondaryPort,
    /// Test the controller itself
    TestController,
    /// Test the primary PS/2 port
    TestPrimaryPort,

    /// Read all bytes of the controller's memory
    DiagnosticDump,

    /// Disable the primary PS/2 port
    DisablePrimaryPort,
    /// Enable the primary PS/2 port
    EnablePrimaryPort,

    /// Read the controller input port
    ReadControllerInputPort,

    /// Copy bits 0 to 3 of the input port to status register bits 4 to 7
    CopyLow,
    /// Copy bits 4 to 7 of the input port to status register bits 4 to 7
    CopyHigh,

    /// Read from the controller output port
    ReadControllerOutputPort,
    /// Write to the controller output port
    WriteControllerOutputPort,

    /// Write a byte to the controller which will then appear as if it had come from the primary controller
    FakePrimaryRead,
    /// Write a byte to the controller which will then appear as if it had come from the secondary controller
    FakeSecondaryRead,

    /// Write a byte to the secondary PS/2 port
    SecondaryWrite,

    /// Pulses an output line for 6ms.
    /// The parameter is a bit flag for which lines are pulsed.
    /// Bit 0 is the reset line, and bits 1 to 3 are other lines with non-standard functions.
    /// To pulse just the reset line, use [`PULSE_RESET_LINE`]
    /// 
    /// [`PULSE_RESET_LINE`]: Ps2ControllerCommand::PULSE_RESET_LINE
    PulseOutputLine(u8),
}

impl Ps2ControllerCommand {
    /// Command to pulse only the reset line
    #[allow(dead_code)]
    const PULSE_RESET_LINE: Self = Self::PulseOutputLine(1);

    /// Gets the byte which needs to be written to the command register in order to execute this command
    fn as_u8(&self) -> u8 {
        match self {
            Ps2ControllerCommand::ReadByte(b) => {
                assert_eq!(b & 0b11111, 0);
                0x20 | b
            }
            Ps2ControllerCommand::WriteByte(b) => {
                assert_eq!(b & 0b11111, 0);
                0x60 | b
            }
            Ps2ControllerCommand::DisableSecondaryPort => 0xA7,
            Ps2ControllerCommand::EnableSecondaryPort => 0xA8,
            Ps2ControllerCommand::TestSecondaryPort => 0xA9,
            Ps2ControllerCommand::TestController => 0xAA,
            Ps2ControllerCommand::TestPrimaryPort => 0xAB,
            Ps2ControllerCommand::DiagnosticDump => 0xAC,
            Ps2ControllerCommand::DisablePrimaryPort => 0xAD,
            Ps2ControllerCommand::EnablePrimaryPort => 0xAE,
            Ps2ControllerCommand::ReadControllerInputPort => 0xC0,
            Ps2ControllerCommand::CopyLow => 0xC1,
            Ps2ControllerCommand::CopyHigh => 0xC2,
            Ps2ControllerCommand::ReadControllerOutputPort => 0xD0,
            Ps2ControllerCommand::WriteControllerOutputPort => 0xD1,
            Ps2ControllerCommand::FakePrimaryRead => 0xD2,
            Ps2ControllerCommand::FakeSecondaryRead => 0xD3,
            Ps2ControllerCommand::SecondaryWrite => 0xD4,
            Ps2ControllerCommand::PulseOutputLine(lines) => {
                assert_eq!(lines & 0b1111, 0);
                0xF0 | lines
            }
        }
    }

    /// Gets the timeout error associated with a test command
    fn get_timeout_error(&self) -> Ps2ControllerInitialisationError {
        match self {
            Self::TestController => Ps2ControllerInitialisationError::ControllerTestFailed,
            Self::TestPrimaryPort => Ps2ControllerInitialisationError::PortTestFailed(
                Ps2Port::Primary,
                Ps2PortTestFailureError::NoResponse,
            ),
            Self::TestSecondaryPort => Ps2ControllerInitialisationError::PortTestFailed(
                Ps2Port::Secondary,
                Ps2PortTestFailureError::NoResponse,
            ),
            _ => panic!("Command is not a test command"),
        }
    }
}

/// A command which can be sent to a PS/2 device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ps2DeviceCommand {
    /// Resets the device. This command will usually return a status code of 0xFA for success or 0xFC for error.
    Reset,
    /// Disables the device sending user input
    DisableScanning,
    /// Enables the device sending user input
    EnableScanning,
    /// Causes the device to send bytes identifying what kind of device it is
    Identify,
}

impl Ps2DeviceCommand {
    /// Converts the command to the byte to send to the controller
    fn to_u8(self) -> u8 {
        match self {
            Self::Reset => 0xFF,
            Self::DisableScanning => 0xF5,
            Self::EnableScanning => 0xF4,
            Self::Identify => 0xF2,
        }
    }
}
