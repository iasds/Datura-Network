/*!
Constants for use in resource allocation
*/

//every rung equals one order magnitude more of difficulty, here we are effectively capping max
//at 54 bits targets, the higher the more units will be created but the hardest it will be to reach
pub const MAX_RUNG: u32 = 10;
