contract callee {
	function get() public pure returns (uint256) {
		return 1;
	}
}

contract calls {
	function plain(callee c) public pure returns (uint256) {
		return c.get();
	}

	function raw(address a) public returns (bool ok) {
		(ok, ) = a.call("");
		(ok, ) = a.staticcall("");
		(ok, ) = a.delegatecall("");
	}
}
