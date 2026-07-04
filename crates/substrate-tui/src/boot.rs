use ratatui::{layout::{Alignment,Constraint,Direction,Layout},style::{Color,Modifier,Style},text::{Line,Span},widgets::{Block,Borders,Gauge,List,ListItem,Paragraph},Frame};
const SPINNER: [char; 4] = ['|','/','-','\\'];
#[derive(Debug,Clone,PartialEq)]
pub enum BootPhase { Initializing, ConnectingGateway, LoadingConfig, Ready }
impl BootPhase {
    pub fn label(&self) -> &'static str { match self { Self::Initializing=>"Initializing runtime", Self::ConnectingGateway=>"Connecting to gateway", Self::LoadingConfig=>"Loading configuration", Self::Ready=>"Ready" } }
    pub fn progress(&self) -> u16 { match self { Self::Initializing=>25, Self::ConnectingGateway=>50, Self::LoadingConfig=>75, Self::Ready=>100 } }
    pub fn next(&self) -> Option<Self> { match self { Self::Initializing=>Some(Self::ConnectingGateway), Self::ConnectingGateway=>Some(Self::LoadingConfig), Self::LoadingConfig=>Some(Self::Ready), Self::Ready=>None } }
}
#[derive(Debug)]
pub struct BootState { pub phase: BootPhase, pub elapsed: u32, pub spin: u8, pub done: Vec<BootPhase> }
impl Default for BootState { fn default() -> Self { Self { phase: BootPhase::Initializing, elapsed: 0, spin: 0, done: Vec::new() } } }
impl BootState {
    pub fn tick(&mut self) -> bool { self.elapsed+=1; self.spin=(self.spin+1)%4; if self.elapsed%8==0 { if let Some(n)=self.phase.next() { self.done.push(self.phase.clone()); self.phase=n; if self.phase==BootPhase::Ready { return true; } } } false }
}
pub fn render_boot(f: &mut Frame, s: &BootState) {
    let c = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3),Constraint::Length(3),Constraint::Min(4)]).margin(2).split(f.area());
    f.render_widget(Paragraph::new("SUBSTRATE").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)).alignment(Alignment::Center), c[0]);
    f.render_widget(Gauge::default().block(Block::default().borders(Borders::NONE)).gauge_style(Style::default().fg(Color::Cyan)).percent(s.phase.progress()), c[1]);
    let phases=[BootPhase::Initializing,BootPhase::ConnectingGateway,BootPhase::LoadingConfig];
    let items:Vec<ListItem>=phases.iter().map(|p|{ let done=s.done.contains(p); let cur=&s.phase==p; let sp=SPINNER[s.spin as usize]; let (icon,col)=if done{"✓",Color::Green} else if cur {(Box::leak(format!("{sp}").into_boxed_str()) as &str,Color::Yellow)} else {"○",Color::DarkGray}; ListItem::new(Line::from(Span::styled(format!("{icon} {}",p.label()),Style::default().fg(col)))) }).collect();
    f.render_widget(List::new(items).block(Block::default().borders(Borders::NONE)), c[2]);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn initial() { assert_eq!(BootState::default().phase, BootPhase::Initializing); }
    #[test] fn advances() { let mut b=BootState::default(); for _ in 0..8 { b.tick(); } assert_eq!(b.phase, BootPhase::ConnectingGateway); }
    #[test] fn completes() { let mut b=BootState::default(); let mut ok=false; for _ in 0..100 { if b.tick() { ok=true; break; } } assert!(ok); }
    #[test] fn done_grows() { let mut b=BootState::default(); for _ in 0..16 { b.tick(); } assert!(!b.done.is_empty()); }
    #[test] fn progress_order() { assert!(BootPhase::ConnectingGateway.progress()>BootPhase::Initializing.progress()); }
}
