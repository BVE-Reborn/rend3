# Development notes
John Nagle
nagle@animats.com

Reviving Rend3 after its abandonment.

## 2024-10-12

Changes to WGPU have broken Rend3. The worst change is

https://github.com/gfx-rs/wgpu/pull/5884

which changes the lifetime of "RenderPass" and Encoder to 'static

which generates this problem:

https://users.rust-lang.org/t/unexpected-borrowed-data-escapes-outside-of-closure-for-generic/119501/4

RenderPass and Controller ownership is currently manipulated by unsafe code. 
That needs to be figured out. 

###Trouble spot: take_rpass
  
https://github.com/BVE-Reborn/rend3/blob/d088a841b0469d07d5a7ff3f4d784e97b4a194d5/rend3/src/graph/encpass.rs#L74
  
    pub(super) enum RenderGraphEncoderOrPassInner<'a, 'pass> {
        Encoder(&'a mut CommandEncoder),
        RenderPass(&'a mut RenderPass<'pass>),
        #[default]
        None,
    }   
  
    pub struct RenderGraphEncoderOrPass<'a, 'pass>(pub(super) RenderGraphEncoderOrPassInner<'a, 'pass>);
    ...
    pub fn take_rpass(&mut self, _handle: DeclaredDependency<RenderPassHandle>) -> &'a mut RenderPass<'pass> {
    match mem::take(&mut self.0) {
        
This in itself seems legit, i.e. safe. Except that it's taking a mutable reference, not the object itself.
There can only be one such mutable reference. Where does this come from?
  
Around here:
  
  https://github.com/BVE-Reborn/rend3/blob/d088a841b0469d07d5a7ff3f4d784e97b4a194d5/rend3/src/graph/graph.rs#L451
  
where there is unsafe code to seemingly allow two mutable references to the same object to exist simultaneously.
  
    // SAFETY: There is no active renderpass to borrow this. This reference lasts for the duration of
    // the call to exec.
    None => RenderGraphEncoderOrPassInner::Encoder(unsafe { &mut *encoder_cell.get() }),
      
Note that this is not an assertion that the code here is safe.
It is an unchecked constraint on the rest of the program.
  
Why is this not done with a RefCell and .borrow_mut?
  
Other than the mess around RenderPass, there's not that much unsafe code in rend3.
This might be fixable.
  
Maybe if we pass around something that has a RefCell<RenderGraphEncoderOrPassInner>
to everybody who needs that...
But that encapsulates a mutable reference. Who actually *owns* the thing?
Question: if we put RenderPass and Encoder inside an Rc<RefCell<>>, will that work?
Or will we panic at borrows?
Worth a try.

## 2024-10-13

Who actually owns RenderPass and Encoder?

    RenderGraphEncoderOrPassInner holds an &mut of CommandEncoder or RenderPass

Who actually creates RenderGraphEncoderOrPassInner?
* Created in graph.rs execute

  https://github.com/BVE-Reborn/rend3/blob/d088a841b0469d07d5a7ff3f4d784e97b4a194d5/rend3/src/graph/graph.rs#L451

      RenderGraphEncoderOrPassInner::RenderPass(rpass)

  https://github.com/BVE-Reborn/rend3/blob/d088a841b0469d07d5a7ff3f4d784e97b4a194d5/rend3/src/graph/graph.rs#L455

       None => RenderGraphEncoderOrPassInner::Encoder(unsafe { &mut *encoder_cell.get() }),
 
  Unsafe code. Is this really necessary?
  
  https://github.com/BVE-Reborn/rend3/blob/d088a841b0469d07d5a7ff3f4d784e97b4a194d5/rend3/src/graph/graph.rs#L484
  
  Same situation.
  
  That's a get from an UnsafeCell. That's probably what should be a RefCell.
  The UnsafeCell actually owns the encoder. 
  Who owns the RenderPass? 
  The caller of 
      create_rpass_from_desc
  
### First cut at design:

      create_rpass_from_desc

returns an Rc<RefCell<RenderPass>>.

At line 391 in graph.rs,
  encoder_cell becomes an Rc<RefCell>> of a new command encoder.
  
Similarly for rpass_temps_cell.

RenderGraphEncoderOrPassInner must hold an Rc<RefCell<>> for each item.

So where do we do the borrows?

At the unsafe points? Probably.

What about take_rpass? 
Do the borrow at self.internal.render in lib.rs? 

This will probably all compile and it's all safe code. If the comments about safety are true,
the borrow_mut calls should not fail. We will see.

## 2024-10-14

Fix all other compile errors from Winit changes. [DONE]

## 2024-10-16

Completed fixing Winit problems, with event loop disabled.
Time to do the above changes to Rend3.

Most of it now compiles.

Trouble at line 507 of graph.rs





  
  
