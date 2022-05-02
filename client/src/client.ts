import { loadPackageDefinition, credentials, Metadata } from "@grpc/grpc-js"
import fs from "fs"
import { loadSync } from "@grpc/proto-loader";
import fsPath from "path";
import { createHash } from "crypto";

const PROTO_PATH = fsPath.join(__dirname, '../proto/api.proto'),
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

const client = new apiProto.Service(SERVER_URL, credentials.createInsecure());

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

const downloadFile = () => {
	const call = client.Download(
		{
			location: "e_test/folder/e_3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a/folder21/file.txt",
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

	let call = client.Upload(meta, (err: any, res: { key: string }) => {
		console.log(err, res)
	})

	await call.write(request)

	for (let { stream, cid } of createReadStreams(metaData.children.map(({ name, cid }) => ({ path: fsPath.join(path, name), cid })))) {
		await new Promise((res, rej) => {
			stream.on("data", (chunk) => {
				call.write({ file: { content: chunk, cid } })
			})
			stream.on("end", res)
			stream.on("error", (e) => rej(e.message))
		})
	}

	await call.end()
}

uploadDirectory("../test/Hello").then(() => {
	downloadFile()
})
