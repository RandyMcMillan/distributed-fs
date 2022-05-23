import { loadPackageDefinition, credentials, Metadata } from "@grpc/grpc-js"
import { createHash } from "crypto";
import fs from "fs"
import { loadSync } from "@grpc/proto-loader";
import fsPath from "path";
const CryptoJS = require('crypto-js');

const PROTO_PATH = fsPath.join(__dirname, '../proto/api.proto'),
	SERVER_URL = "192.168.0.248:50051",
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

const client = new apiProto.Service(SERVER_URL, credentials.createInsecure());

const meta = new Metadata();
meta.add('public_key', PUBLIC_KEY);

const clearDir = async (dir: string) => new Promise((res, rej) => {
	const path = fsPath.join(__dirname, dir)
	fs.readdir(path, (err, files) => {
		if (err) rej(err);

		for (const file of files) {
			fs.unlinkSync(fsPath.join(path, file));
		}
		res(null)
	});
})

const downloadFile = async () => {
	const secp256k1 = require('secp256k1')

	// const privKey = Uint8Array.from("3f9a803e135f1e3b36ac7f246ba5b56e164b42074ea1659b0e6d52eeb9950ea1")
	// const privKey = Uint8Array.from(Buffer.from('3f9a803e135f1e3b36ac7f246ba5b56e164b42074ea1659b0e6d52eeb9950ea1', 'utf-8'))
	const privKey = Uint8Array.from([63, 154, 128, 62, 19, 95, 30, 59, 54, 172, 127, 36, 107, 165, 181, 110, 22, 75, 66, 7, 78, 161, 101, 155, 14, 109, 82, 238, 185, 149, 14, 161])

	let m = Buffer.from("e_somelocation/folder/e_3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a/")
	let msg = Buffer.from(createHash('sha256').update(m).digest('hex'))
	console.log(msg.toString("ascii"))

	// let hash = CryptoJS.SHA256(m);
	// let buffer = Buffer.from(hash.toString(CryptoJS.enc.HEX), 'hex');
	// let msg = new Uint8Array(buffer);
	// console.log(msg, buffer.toString("hex"), privKey, hash)


	const { signature: signed } = secp256k1.ecdsaSign(msg, privKey)

	// console.log(Buffer.from(signed).toString("hex"))

	const call = client.Get(
		{
			location: "e_somelocation/folder/e_3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a/",
			name: "Hello",
			download: true
		},
		meta
	)

	await clearDir("../download")
	call.on("data", (res: any) => {
		console.log("DATA: ", res[res.download_response], res)
		if (res.download_response === "file") {
			fs.appendFileSync(`./download/${fsPath.basename(res[res.download_response].name)}`, res[res.download_response].content)
		}
	})

	call.on("end", () => {
		console.log("end")
	})
}

type Type = "file" | "directory"
type Children = { type: Type, name: string, cid: string, entry: null }[]

interface MetaData {
	name: string
	children: Children
}

const getMetaDataForDir = (path: string): null | MetaData => {
	if (!fs.existsSync(path)) return null

	const metaData: MetaData = {
		name: fsPath.basename(path),
		children: []
	}

	const getChildren = (path: string, basePath: string = ""): Children => {
		let children: Children = []

		fs.readdirSync(path).forEach(entry => {
			let entryPath = fsPath.join(path, entry)

			if (fs.lstatSync(entryPath).isDirectory())
				children = [...children, ...getChildren(entryPath, fsPath.join(basePath, entry))]
			else {
				let data = fs.readFileSync(fsPath.join(path, entry))

				let hasher = createHash("sha256")
				hasher.update(data)

				let cid = hasher.digest("hex");

				children.push({
					name: fsPath.join(basePath, entry),
					type: "file",
					cid,
					entry: null
				})
			}
		})

		return children
	}

	metaData.children = getChildren(path)

	return metaData
}

const createReadStreams = (paths: { path: string, cid: string }[]) => (paths.map(
	({ path, cid }) => ({ cid, stream: fs.createReadStream(fsPath.join(__dirname, path), { highWaterMark: 1024 * 128 }) })
))

const uploadDirectory = async (path: string) => {
	let isPublic = true,
		read_users: string[] = []

	const metaData = getMetaDataForDir(fsPath.join(__dirname, path))
	if (!metaData) return

	let entry = {
		owner: PUBLIC_KEY,
		public: isPublic,
		read_users,
		children: metaData?.children,
		name: metaData.name
	}

	let request = {
		metadata: {
			signature: "3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a",
			entry
		}
	}

	let call = client.Put(meta, (err: any, res: { key: string }) => {
		console.log(err, res)
	})

	await call.write(request)

	for (let { stream, cid } of createReadStreams(metaData.children.map(({ name, cid }) => ({ path: fsPath.join(path, name), cid })))) {
		await new Promise((res, rej) => {
			stream.on("data", (chunk) => {
				call.write({ file: { content: chunk, cid } })
			})
			stream.on("end", res)
			stream.on("error", (e) => console.log("TEST", e.message))
		})
	}

	await call.end()
}

uploadDirectory("../test/Hello").then(() => {
	downloadFile()
})
