import { loadPackageDefinition, credentials, Metadata } from "@grpc/grpc-js"
import fs from "fs"
import { loadSync } from "@grpc/proto-loader";
import path from "path";
// import { createHash, randomBytes } from "crypto";

// import { SHA256, enc } from "crypto-js"
// import secp256k1 from "secp256k1"
// @ts-ignore
// import secp256k1 from "secp256k1-native";
// import { ec as EC } from "elliptic"





const PROTO_PATH = path.join(__dirname, '../proto/api.proto'),
	SERVER_URL = "192.168.0.164:50051",
	PUBLIC_KEY = "023887a11113c0c72d1f887794490e70ad0f0f7cf81ab43de2998cbdab5b7bfd5a",
	PRIVATE_KEY = "4b3bee129b6f2a9418d1a617803913e3fee922643c628bc8fb48e0b189d104de"

const packageDefinition = loadSync(
	PROTO_PATH,
	{
		keepCase: true,
		longs: String,
		enums: String,
		defaults: true,
		oneofs: true,
	}
);

const apiProto = loadPackageDefinition(packageDefinition).api as any;

const client = new apiProto.Api(SERVER_URL, credentials.createInsecure());

const meta = new Metadata();
meta.add('public_key', PUBLIC_KEY);

const getEntry = (location: string) => {
	client.Get(
		{
			location
		},
		meta,
		(err: any, data: any) => {
			console.log(data, err)
		}
	)
}

const putEntry = () => {
	client.Put(
		{
			signature: "3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a",
			entry: {
				owner: "owner",
				public: true,
				read_users: [
					"User"
				],
				name: "Hello"
			}
		},
		meta,
		(err: any, data: any) => {
			console.log(data, err)
		}
	)

}

const uploadFile = () => {
	let call = client.Upload(meta, (err: any, res: any) => {
		console.log(err, res)

		res?.key && getEntry(res.key)
	})
	let first = true;


	const input = fs
		// .createReadStream(path.join(__dirname, "../pfp.png"));
		.createReadStream(path.join(__dirname, "../../Cargo.toml"), { highWaterMark: 1024 * 128 });

	// input.pipe(split())
	input
		.on("data", (chunk) => {
			// call.write({ file: { content: Buffer.from(chunk, "utf-8") } })
			if (first) {
				// const ec = new EC('secp256k1');
				// let key = ec.keyFromPrivate(PRIVATE_KEY)

				// let sig = key.sign("test")
				// let signature = sig.toDER("hex")
				// console.log(signature, Buffer.from(chunk).toJSON())

				// let enc = new TextEncoder()
				// let privkey = enc.encode(PRIVATE_KEY)
				// let msg = new Uint8Array(Buffer.from(chunk))
				// const sigObj = secp256k1.ecdsaSign(msg, privkey)
				// console.log(sigObj)


				// let encoder = new TextEncoder()
				// let privkey = encoder.encode(PRIVATE_KEY)
				// let msg = new Uint8Array(Buffer.from(createHash('sha256').update(Buffer.from(chunk)).digest('hex')))
				// console.log(msg.length, createHash('sha256').update(Buffer.from(chunk)).digest('hex'))
				// const sigObj = secp256k1.ecdsaSign(msg, privkey)
				// console.log(sigObj)

				// let hash = SHA256(Buffer.from(chunk).toString());
				// let buffer = Buffer.from(hash.toString(enc.Hex), 'hex');
				// let array = new Uint8Array(buffer);

				// console.log(array.length, privkey, privkey.length)
				// const sigObj = secp256k1.ecdsaSign(array, privkey)
				// console.log(sigObj)


				// const signCtx = secp256k1.secp256k1_context_create(secp256k1.secp256k1_context_SIGN)

				// let seckey
				// // do {
				// // 	seckey = randomBytes(32)
				// // 	// seckey = Buffer.from(PRIVATE_KEY)
				// // } while (!secp256k1.secp256k1_ec_seckey_verify(signCtx, seckey))
				// console.log(seckey, Buffer.from(PRIVATE_KEY))

				// const pubkey = Buffer.alloc(64)
				// secp256k1.secp256k1_ec_pubkey_create(signCtx, pubkey, seckey)

				// const msg = Buffer.from(chunk)
				// const msg32 = createHash('sha256').update(msg).digest()

				// const sig = Buffer.alloc(64)
				// secp256k1.secp256k1_ecdsa_sign(signCtx, sig, msg32, seckey)

				// console.log(sig.toString("hex"))



				call.write({
					metadata: {
						signature: "3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a",
						entry: {
							owner: "owner",
							public: true,
							read_users: [
								"User"
							],
							children: [
								{
									name: "file.txt",
									type: "file",
									entry_location: null
								}
							],
							name: "Hello"
						},
					},
				});
				first = false;
				call.write({ file: { content: chunk } })
			} else
				call.write({ file: { content: chunk } })
		})
		.on("end", () =>
			call.end()
		)
}

const downloadFile = () => {
	const call = client.Download(
		{
			location: "e_304402205cb2bf1b2619f84bf9e88f1b288ad47b982a569d6921d0f62d2548062b6fedb902207043d35fe41b6b272b12379ba04db1b2c68731b36f3f038182e4c6d8aad4850d",
			name: "Hello",
			download: true
		},
		meta
	)

	if (fs.existsSync("./download")) fs.rmSync("./download")
	call.on("data", (res: any) => {
		console.log("DATA: ", res[res.download_response], res)
		if (res.download_response === "file") {
			fs.appendFileSync("./download", res[res.download_response].content)
		}
	})

	call.on("end", () => {
		console.log("end")
	})
}

uploadFile()
//downloadFile()
// putEntry()
// getEntry("e_304402205cb2bf1b2619f84bf9e88f1b288ad47b982a569d6921d0f62d2548062b6fedb902207043d35fe41b6b272b12379ba04db1b2c68731b36f3f038182e4c6d8aad4850d")
