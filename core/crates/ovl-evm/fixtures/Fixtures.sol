// SPDX-License-Identifier: MIT
// Compiled with solc 0.8.30 --evm-version shanghai (optimizer on, 200 runs). These are executable
// fixtures, not audited production deployments and not protocol balances.
pragma solidity 0.8.30;

contract Store {
    uint256 public value;
    event Stored(uint256 v);
    function set(uint256 v) external {
        value = v;
        emit Stored(v);
    }
}

contract Erc20 {
    string public name = "Fixture";
    string public symbol = "FIX";
    uint8 public decimals = 18;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    constructor(uint256 supply) {
        totalSupply = supply;
        balanceOf[msg.sender] = supply;
        emit Transfer(address(0), msg.sender, supply);
    }
    function transfer(address to, uint256 amount) external returns (bool) {
        require(balanceOf[msg.sender] >= amount, "bal");
        unchecked {
            balanceOf[msg.sender] -= amount;
            balanceOf[to] += amount;
        }
        emit Transfer(msg.sender, to, amount);
        return true;
    }
    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }
    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        require(balanceOf[from] >= amount, "bal");
        require(allowance[from][msg.sender] >= amount, "allow");
        unchecked {
            allowance[from][msg.sender] -= amount;
            balanceOf[from] -= amount;
            balanceOf[to] += amount;
        }
        emit Transfer(from, to, amount);
        return true;
    }
}

contract Erc721 {
    mapping(uint256 => address) public ownerOf;
    mapping(address => uint256) public balanceOf;
    mapping(uint256 => address) public getApproved;
    event Transfer(address indexed from, address indexed to, uint256 indexed id);
    function mint(address to, uint256 id) external {
        require(ownerOf[id] == address(0), "exists");
        ownerOf[id] = to;
        balanceOf[to] += 1;
        emit Transfer(address(0), to, id);
    }
    function approve(address spender, uint256 id) external {
        require(ownerOf[id] == msg.sender, "owner");
        getApproved[id] = spender;
    }
    function transferFrom(address from, address to, uint256 id) external {
        require(ownerOf[id] == from, "from");
        require(from == msg.sender || getApproved[id] == msg.sender, "auth");
        balanceOf[from] -= 1;
        balanceOf[to] += 1;
        ownerOf[id] = to;
        getApproved[id] = address(0);
        emit Transfer(from, to, id);
    }
}

contract Erc1155 {
    mapping(address => mapping(uint256 => uint256)) public balanceOf;
    event TransferSingle(address indexed op, address indexed from, address indexed to, uint256 id, uint256 value);
    function mint(address to, uint256 id, uint256 amount) external {
        balanceOf[to][id] += amount;
        emit TransferSingle(msg.sender, address(0), to, id, amount);
    }
    function safeTransferFrom(address from, address to, uint256 id, uint256 amount, bytes calldata) external {
        require(from == msg.sender, "auth");
        require(balanceOf[from][id] >= amount, "bal");
        unchecked {
            balanceOf[from][id] -= amount;
            balanceOf[to][id] += amount;
        }
        emit TransferSingle(msg.sender, from, to, id, amount);
    }
}

contract Erc4626 {
    Erc20 public immutable asset;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Deposit(address indexed caller, address indexed owner, uint256 assets, uint256 shares);
    event Withdraw(address indexed caller, address indexed receiver, address indexed owner, uint256 assets, uint256 shares);
    constructor(Erc20 asset_) {
        asset = asset_;
    }
    function deposit(uint256 assets, address receiver) external returns (uint256 shares) {
        shares = assets;
        require(asset.transferFrom(msg.sender, address(this), assets), "pull");
        totalSupply += shares;
        balanceOf[receiver] += shares;
        emit Transfer(address(0), receiver, shares);
        emit Deposit(msg.sender, receiver, assets, shares);
    }
    function withdraw(uint256 assets, address receiver, address owner) external returns (uint256 shares) {
        shares = assets;
        require(msg.sender == owner, "owner");
        require(balanceOf[owner] >= shares, "shares");
        balanceOf[owner] -= shares;
        totalSupply -= shares;
        emit Transfer(owner, address(0), shares);
        require(asset.transfer(receiver, assets), "push");
        emit Withdraw(msg.sender, receiver, owner, assets, shares);
    }
}

contract MultiSig {
    address public ownerA;
    address public ownerB;
    mapping(bytes32 => uint8) public confirms;
    event Executed(bytes32 indexed id);
    constructor(address a, address b) {
        ownerA = a;
        ownerB = b;
    }
    function confirm(address target, bytes calldata data) external {
        require(msg.sender == ownerA || msg.sender == ownerB, "owner");
        bytes32 id = keccak256(abi.encode(target, data));
        if (msg.sender == ownerA) confirms[id] |= 1;
        if (msg.sender == ownerB) confirms[id] |= 2;
    }
    function execute(address target, bytes calldata data) external {
        bytes32 id = keccak256(abi.encode(target, data));
        require(confirms[id] == 3, "quorum");
        confirms[id] = 0;
        (bool ok,) = target.call(data);
        require(ok, "call");
        emit Executed(id);
    }
}

contract Counter {
    uint256 public value;
    function inc() external {
        value += 1;
    }
}

contract Proxy {
    // Implementation lives outside slot 0 so delegatecall storage matches the
    // implementation layout instead of overwriting it.
    uint256 private constant SLOT = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;
    constructor(address impl_) {
        assembly {
            sstore(SLOT, impl_)
        }
    }
    fallback() external payable {
        assembly {
            let impl_ := sload(SLOT)
            calldatacopy(0, 0, calldatasize())
            let ok := delegatecall(gas(), impl_, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch ok
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }
}

contract Timelock {
    mapping(bytes32 => bool) public queued;
    function queue(address target, bytes calldata data, uint256 eta) external {
        require(eta >= block.timestamp, "eta");
        bytes32 id = keccak256(abi.encode(target, data, eta));
        queued[id] = true;
    }
    function execute(address target, bytes calldata data, uint256 eta) external {
        bytes32 id = keccak256(abi.encode(target, data, eta));
        require(queued[id], "missing");
        require(block.timestamp >= eta, "early");
        queued[id] = false;
        (bool ok,) = target.call(data);
        require(ok, "call");
    }
}

contract Amm {
    uint256 public reserve0;
    uint256 public reserve1;
    function add(uint256 a, uint256 b) external {
        reserve0 += a;
        reserve1 += b;
    }
    function swap0for1(uint256 amountIn) external returns (uint256 amountOut) {
        uint256 inWithFee = amountIn * 997;
        amountOut = (inWithFee * reserve1) / (reserve0 * 1000 + inWithFee);
        require(amountOut > 0 && amountOut < reserve1, "out");
        reserve0 += amountIn;
        reserve1 -= amountOut;
    }
}

contract Create2Factory {
    function deploy(bytes memory code, bytes32 salt) external returns (address addr) {
        assembly {
            addr := create2(0, add(code, 0x20), mload(code), salt)
        }
        require(addr != address(0), "create2");
    }
}

contract StaticProbe {
    function echo(uint256 x) external pure returns (uint256) {
        return x;
    }
    function read(address target) external view returns (uint256) {
        (bool ok, bytes memory ret) = target.staticcall(
            abi.encodeWithSignature("echo(uint256)", uint256(7))
        );
        require(ok, "static");
        return abi.decode(ret, (uint256));
    }
}

contract PrecompileProbe {
    function keccakOf(bytes memory data) external pure returns (bytes32) {
        return keccak256(data);
    }
    function sha256Of(bytes memory data) external pure returns (bytes32) {
        return sha256(data);
    }
    function ripemdOf(bytes memory data) external view returns (bytes20) {
        return ripemd160(data);
    }
    function recover(bytes32 hash, uint8 v, bytes32 r, bytes32 s) external pure returns (address) {
        return ecrecover(hash, v, r, s);
    }
}
