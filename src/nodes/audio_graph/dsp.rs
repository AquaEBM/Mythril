use super::*;
use std::simd::f32x2;

#[derive(Default)]
pub struct ProcessSchedule { 
    nodes: Vec<Box<dyn Processor + Send>>,
    buffers: Vec<f32x2>,
    edges: Vec<Vec<usize>>,
}

impl Processor for ProcessSchedule {

    // num_voices: new number of voices
    fn add_voice(&mut self, norm_freq: f32) {

        for processor in self.nodes.iter_mut() {
            processor.add_voice(norm_freq);
        }
    }

    // num_voices: new number of voices
    fn remove_voice(&mut self, voice_idx: usize) {

        for processor in self.nodes.iter_mut() {
            processor.remove_voice(voice_idx);
        }
    }

    fn process(&mut self, _input: f32x2, voice_idx: usize, editor_open: bool) -> f32x2 {

        let mut out = f32x2::splat(0.);

        for (i, (node, edges)) in self.nodes.iter_mut().zip(self.edges.iter()).enumerate() {
            let node_out = node.process(self.buffers[i], voice_idx, editor_open);

            for &edge in edges {
                if edge == usize::MAX {
                    out += node_out;
                } else {
                    self.buffers[edge] += node_out;
                }
            }
        }

        out
    }

    fn initialize(&mut self, sample_rate: f32) -> (bool, u32) {
        self.nodes.iter_mut().for_each(|node| { node.initialize(sample_rate); });
        (true, 0)
    }

    fn reset(&mut self) {
        self.nodes.iter_mut().for_each(|node| node.reset());
    }

    fn update_smoothers(&mut self) {
        self.nodes.iter_mut()
            .map(AsMut::as_mut)
            .for_each(Processor::update_smoothers)
    }
}

impl ProcessSchedule {
    pub(super) fn push(
        &mut self, processor: Box<dyn Processor + Send>,
        outputs: Vec<usize>,
    ) {
        self.buffers.push(f32x2::splat(0.));
        self.nodes.push(processor.into());
        self.edges.push(outputs);
    }
}

impl SeenthStandAlonePlugin for SeenthParams {
    type Processor = ProcessSchedule;

    fn processor(self: Arc<Self>) -> Self::Processor {
        self.schedule()
    }

    fn editor_state(&self) -> Arc<EguiState> {
        self.editor_state.clone()
    }
}
