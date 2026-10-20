contract balances {
	mapping(address => uint256) balance;

	function credit(address to, uint256 amount) public {
		balance[to] += amount;
	}

	function get(address who) public view returns (uint256) {
		return balance[who];
	}
}
