import { loadPackageDefinition, credentials, Metadata } from "@grpc/grpc-js"
import fs from "fs"
import { loadSync } from "@grpc/proto-loader";
import path from "path";
import split from "split"

const PROTO_PATH = path.join(__dirname, '../proto/api.proto'),
	SERVER_URL = "192.168.0.164:50051"

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
meta.add('public_key', '03efb57d7e730b13ca6b23822f4ecbeb7926a529e02befe4e4a8034c56ce607670');

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
			signature: "304402205cb2bf1b2619f84bf9e88f1b288ad47b982a569d6921d0f62d2548062b6fedb902207043d35fe41b6b272b12379ba04db1b2c68731b36f3f038182e4c6d8aad4850d",
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

	call.write({
		metadata: {
			signature: "304402205cb2bf1b2619f84bf9e88f1b288ad47b982a569d6921d0f62d2548062b6fedb902207043d35fe41b6b272b12379ba04db1b2c68731b36f3f038182e4c6d8aad4850d",
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
			}
		},
	})

	const input = fs
		// .createReadStream(path.join(__dirname, "../pfp.png"));
		.createReadStream(path.join(__dirname, "../img.png"), { highWaterMark: 1024 * 128 });

	// input.pipe(split())
	input
		.on("data", (chunk) => {
			// call.write({ file: { content: Buffer.from(chunk, "utf-8") } })
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
			name: "Hello"
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

// uploadFile()
downloadFile()
// putEntry()
// getEntry("e_304402205cb2bf1b2619f84bf9e88f1b288ad47b982a569d6921d0f62d2548062b6fedb902207043d35fe41b6b272b12379ba04db1b2c68731b36f3f038182e4c6d8aad4850d")
