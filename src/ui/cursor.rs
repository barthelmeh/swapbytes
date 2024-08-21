#[derive(Default)]
pub struct Cursor {
    pub pos: usize,
}

impl Cursor {
    pub fn move_cursor_left(&mut self, size_of_input: usize) {
        let cursor_moved_left = self.pos.saturating_sub(1);
        self.pos = self.clamp_cursor(cursor_moved_left, size_of_input);
    }

    pub fn move_cursor_right(&mut self, size_of_input: usize) {
        let cursor_moved_right = self.pos.saturating_add(1);
        self.pos = self.clamp_cursor(cursor_moved_right, size_of_input);
    }

    pub fn clamp_cursor(&self, new_cursor_pos: usize, size_of_input: usize) -> usize {
        new_cursor_pos.clamp(0, size_of_input)
    }

    pub fn reset_cursor(&mut self) {
        self.pos = 0;
    }
}
