use std::fs::File;
use std::io::{BufReader, Read};
use std::thread;
use std::time::{Duration, Instant};
use pixels::{Pixels, SurfaceTexture};
use rand::Rng;
use winit::dpi::LogicalSize;
use winit::event::{Event, VirtualKeyCode};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;
use winit_input_helper::WinitInputHelper;

struct State {
    display: [bool; 64 * 32],
    registers: [u8; 16],
    memory: [u8; 4096],
    keys: [bool; 16],
    pc: u16,
    index: u16,
    stack: Vec<u16>,
    delay: f64,
    last_time: Instant,
    delay_timer: u8,
    sound_timer: u8,
    i: u8,
}

struct Instruction {
    nibbles: [u8; 4],
}

fn get_nnn_from_instruction(instruction: &Instruction) -> u16 {
    ((instruction.nibbles[1] as u16) << 8) | ((instruction.nibbles[2] as u16) << 4) | instruction.nibbles[3] as u16
}

fn get_nn_from_instruction(instruction: &Instruction) -> u8 {
    (instruction.nibbles[2] << 4) | instruction.nibbles[3]
}

impl Instruction {
    fn do_instruction(&self, state: &mut State) {
        match self.nibbles {
            [0x0, 0x0, 0x0, 0x0] => println!("Run external subroutine."),
            [0x0, 0x0, 0xE, 0x0] => (*state).display = [false; 64 * 32],
            [0x0, 0x0, 0xE, 0xE] => {
                (*state).pc = (*state).stack.pop().unwrap();
            },
            [0x1, _, _, _] => (*state).pc = get_nnn_from_instruction(self),
            [0x2, _, _, _] => {
                (*state).stack.push((*state).pc);
                (*state).pc = get_nnn_from_instruction(self);
            },
            [0x3, _, _, _] => {
                if (*state).registers[(*self).nibbles[1] as usize] == get_nn_from_instruction(self) {
                    (*state).pc += 2;
                }
            }
            [0x4, _, _, _] => {
                if (*state).registers[(*self).nibbles[1] as usize] != get_nn_from_instruction(self) {
                    (*state).pc += 2;
                }
            }
            [0x5, _, _, _] => {
                if (*state).registers[(*self).nibbles[1] as usize] == (*state).registers[(*self).nibbles[2] as usize] {
                    (*state).pc += 2;
                }
            }
            [0x6, _, _, _] => (*state).registers[self.nibbles[1] as usize] = get_nn_from_instruction(self),
            [0x7, _, _, _] => {
                let (result, _) = (*state).registers[self.nibbles[1] as usize].overflowing_add(get_nn_from_instruction(self));
                (*state).registers[self.nibbles[1] as usize] = result;
            },
            [0x8, _, _, 0] => (*state).registers[self.nibbles[1] as usize] = (*state).registers[self.nibbles[2] as usize],
            [0x8, _, _, 1] => (*state).registers[self.nibbles[1] as usize] |= (*state).registers[self.nibbles[2] as usize],
            [0x8, _, _, 2] => (*state).registers[self.nibbles[1] as usize] &= (*state).registers[self.nibbles[2] as usize],
            [0x8, _, _, 3] => (*state).registers[self.nibbles[1] as usize] ^= (*state).registers[self.nibbles[2] as usize],
            [0x8, _, _, 4] => {
                let (result, overflow) = (*state).registers[self.nibbles[1] as usize].overflowing_add((*state).registers[self.nibbles[2] as usize]);
                (*state).registers[self.nibbles[1] as usize] = result;
                (*state).registers[0xF] = overflow as u8;
            }
            [0x8, _, _, 5] => {
                let (result, overflow) = (*state).registers[self.nibbles[1] as usize].overflowing_sub((*state).registers[self.nibbles[2] as usize]);
                (*state).registers[self.nibbles[1] as usize] = result;
                (*state).registers[0xF] = !overflow as u8;
            }
            [0x8, _, _, 6] => {
                (*state).registers[0xF] = (*state).registers[self.nibbles[1] as usize] & 0x1;
                (*state).registers[self.nibbles[1] as usize] >>= 1;
            }
            [0x8, _, _, 7] => {
                let (result, overflow) = (*state).registers[self.nibbles[2] as usize].overflowing_sub((*state).registers[self.nibbles[1] as usize]);
                (*state).registers[self.nibbles[1] as usize] = result;
                (*state).registers[0xF] = !overflow as u8;
            }
            [0x8, _, _, 0xE] => {
                (*state).registers[0xF] = (*state).registers[self.nibbles[1] as usize] >> 7;
                (*state).registers[self.nibbles[1] as usize] <<= 1;
            }
            [0x9, _, _, _] => {
                if (*state).registers[(*self).nibbles[1] as usize] != (*state).registers[(*self).nibbles[2] as usize] {
                    (*state).pc += 2;
                }
            }
            [0xA, _, _, _] => (*state).index = get_nnn_from_instruction(self),
            [0xB, _, _, _] => {
                (*state).pc = get_nnn_from_instruction(self) + (*state).registers[0] as u16;
            }
            [0xC, _, _, _] => {
                let mut rng = rand::thread_rng();
                (*state).registers[self.nibbles[1] as usize] = rng.gen::<u8>() & get_nn_from_instruction(self);
            }
            [0xD, _, _, _] => {
                let x = (*state).registers[self.nibbles[1] as usize];
                let y = (*state).registers[self.nibbles[2] as usize];
                let height = self.nibbles[3];
                let mut pixel_index = (y as usize) * 64 + x as usize;
                let mut sprite_index = (*state).index as usize;
                for _ in 0..height {
                    let sprite_byte = (*state).memory[sprite_index];
                    for i in 0..8 {
                        let sprite_bit = (sprite_byte >> (7 - i)) & 1;
                        if sprite_bit == 1 {
                            (*state).display[pixel_index] = !(*state).display[pixel_index];
                        }
                        pixel_index += 1;
                    }
                    pixel_index -= 8;
                    pixel_index += 64;
                    sprite_index += 1;
                }
            }
            [0xE, _, 9, 0xE] => {
                if (*state).keys[(*state).registers[(*self).nibbles[1] as usize] as usize] {
                    (*state).pc += 2;
                }
            }
            [0xE, _, 0xA, 1] => {
                if !(*state).keys[(*state).registers[(*self).nibbles[1] as usize] as usize] {
                    (*state).pc += 2;
                }
            }
            [0xF, _, 0, 7] => (*state).registers[self.nibbles[1] as usize] = (*state).delay_timer,
            [0xF, _, 0, 0xA] => {
                let mut key_pressed = false;
                for i in 0..16 {
                    if (*state).keys[i] {
                        (*state).registers[self.nibbles[1] as usize] = i as u8;
                        key_pressed = true;
                        break;
                    }
                }
                if !key_pressed {
                    (*state).pc -= 2;
                }
            }
            [0xF, _, 1, 5] => (*state).delay_timer = (*state).registers[self.nibbles[1] as usize],
            [0xF, _, 1, 8] => (*state).sound_timer = (*state).registers[self.nibbles[1] as usize],
            [0xF, _, 1, 0xE] => (*state).index += (*state).registers[self.nibbles[1] as usize] as u16,
            [0xF, _, 2, 9] => (*state).index = (*state).registers[self.nibbles[1] as usize] as u16 * 5,
            [0xF, _, 3, 3] => {
                let value = (*state).registers[self.nibbles[1] as usize];
                (*state).memory[(*state).index as usize] = value / 100;
                (*state).memory[(*state).index as usize + 1] = (value / 10) % 10;
                (*state).memory[(*state).index as usize + 2] = value % 10;
            },
            [0xF, _, 5, 5] => {
                for i in 0..(self.nibbles[1] + 1) {
                    (*state).memory[(*state).index as usize + i as usize] = (*state).registers[i as usize];
                }
            }
            [0xF, _, 6, 5] => {
                for i in 0..(self.nibbles[1] + 1) {
                    (*state).registers[i as usize] = (*state).memory[(*state).index as usize + i as usize];
                }
            }
            [_, _, _, _] => {}
        }
    }
}

impl State {
    fn update(&mut self) {
        let instruction = Instruction {
            nibbles: [
                self.memory[self.pc as usize] >> 4,
                self.memory[self.pc as usize] & 0xF,
                self.memory[self.pc as usize + 1] >> 4,
                self.memory[self.pc as usize + 1] & 0xF,
            ],
        };
        self.pc += 2;
        self.i += 1;
        if self.i % 10 == 0 {
            self.i = 0;
            if self.delay_timer > 0 {
                self.delay_timer -= 1;
            }
            if self.sound_timer > 0 {
                self.sound_timer -= 1;
            }
            // print_screen(self.display);
        }
        instruction.do_instruction(self);
        if self.last_time.elapsed().as_secs_f64() > self.delay {
            self.last_time = Instant::now();
        } else {
            thread::sleep(Duration::from_secs_f64(self.delay - self.last_time.elapsed().as_secs_f64()));
        }
    }

    fn draw(&self, frame: &mut [u8]) {
        for (i, pixel) in frame.chunks_exact_mut(4).enumerate() {
            let x = (i % 64 as usize) as i16;
            let y = (i / 64 as usize) as i16;

            let rgba = if self.display[(x + y * 64) as usize] {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 255]
            };

            pixel.copy_from_slice(&rgba);
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("no path given");
    let f = File::open(std::path::PathBuf::from(path)).unwrap();
    let mut reader = BufReader::new(f);
    let mut buffer = Vec::new();

    let mut state = State {
        display: [false; 64 * 32],
        registers: [0; 16],
        memory: [0; 4096],
        keys: [false; 16],
        pc: 0x200,
        index: 0,
        stack: vec![],
        delay: 1f64 / 6000f64,
        last_time: Instant::now(),
        delay_timer: 0,
        sound_timer: 0,
        i: 0,
    };

    state.memory[0x200..0x200 + reader.read_to_end(&mut buffer).unwrap()].copy_from_slice(&buffer);
    state.memory[0..80].copy_from_slice(&[
        0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
        0x20, 0x60, 0x20, 0x20, 0x70, // 1
        0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
        0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
        0x90, 0x90, 0xF0, 0x10, 0x10, // 4
        0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
        0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
        0xF0, 0x10, 0x20, 0x40, 0x40, // 7
        0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
        0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
        0xF0, 0x90, 0xF0, 0x90, 0x90, // A
        0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
        0xF0, 0x80, 0x80, 0x80, 0xF0, // C
        0xE0, 0x90, 0x90, 0x90, 0xE0, // D
        0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
        0xF0, 0x80, 0xF0, 0x80, 0x80, // F
    ]);

    let event_loop = EventLoop::new();
    let window = {
        let size = LogicalSize::new(64f64 * 20f64, 32f64 * 20f64);
        WindowBuilder::new()
            .with_title("Chip-8 Simulator")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .build(&event_loop)
            .unwrap()
    };
    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        Pixels::new(64, 32, surface_texture).unwrap()
    };

    event_loop.run(move |event, _, control_flow| {
        if let Event::RedrawRequested(_) = event {
            state.draw(pixels.get_frame_mut());
            if let Err(err) = pixels.render() {
                println!("pixels.render() failed: {err}");
                *control_flow = ControlFlow::Exit;
                return;
            }
        }
        let mut input = WinitInputHelper::new();

        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::F1) || input.quit() {
                *control_flow = ControlFlow::Exit;
                println!("Quitting");
                return;
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                if let Err(err) = pixels.resize_surface(size.width, size.height) {
                    println!("pixels.resize_surface() failed: {err}");
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            // Update internal state and request a redraw
            state.update();
            window.request_redraw();
        }
    });
}
